//! An input adapter that generates Nexmark event input data.

use std::collections::VecDeque;
use std::io::Cursor;
use std::mem;
use std::sync::{atomic::Ordering, Arc, Mutex};
use std::sync::{Barrier, OnceLock, Weak};
use std::thread::{self, Thread};

use crate::format::InputBuffer;
use crate::{
    InputConsumer, InputEndpoint, InputReader, Parser, PipelineState, TransportInputEndpoint,
};
use anyhow::{anyhow, Result as AnyResult};
use atomic::Atomic;
use csv::{Writer as CsvWriter, WriterBuilder as CsvWriterBuilder};
use dbsp_nexmark::generator::{NexmarkGenerator, NextEvent};
use dbsp_nexmark::model::Event;
use dbsp_nexmark::{config::GeneratorOptions, generator::config::Config as GeneratorConfig};
use enum_map::EnumMap;
use feldera_types::program_schema::Relation;
use feldera_types::transport::nexmark::{NexmarkInputConfig, NexmarkInputOptions, NexmarkTable};
use rand::rngs::ThreadRng;
use rmpv::Value as RmpValue;

use super::{InputReaderCommand, NonFtInputReaderCommand};

pub(crate) struct NexmarkEndpoint {
    config: NexmarkInputConfig,
}

impl NexmarkEndpoint {
    pub(crate) fn new(config: NexmarkInputConfig) -> Self {
        Self { config }
    }
}

impl InputEndpoint for NexmarkEndpoint {
    fn is_fault_tolerant(&self) -> bool {
        false
    }
}

impl TransportInputEndpoint for NexmarkEndpoint {
    fn open(
        &self,
        consumer: Box<dyn InputConsumer>,
        parser: Box<dyn Parser>,
        _schema: Relation,
    ) -> AnyResult<Box<dyn InputReader>> {
        Ok(Box::new(InputGenerator::new(
            &self.config,
            consumer,
            parser,
        )?))
    }
}

struct InputGenerator {
    table: NexmarkTable,
    consumer: Box<dyn InputConsumer>,
    inner: Arc<Inner>,
}

impl InputGenerator {
    pub fn new(
        config: &NexmarkInputConfig,
        consumer: Box<dyn InputConsumer>,
        parser: Box<dyn Parser>,
    ) -> AnyResult<Self> {
        let mut guard = INNER.lock().unwrap();
        let inner = guard.upgrade().unwrap_or_else(|| {
            let inner = Inner::new();
            *guard = Arc::downgrade(&inner);
            inner
        });
        drop(guard);

        inner.merge(config, consumer.clone(), parser)?;
        Ok(Self {
            table: config.table,
            consumer,
            inner,
        })
    }

    fn queue(&self) {
        if self.table == NexmarkTable::Bid {
            let mut total = 0;
            let options = self.inner.options.get().unwrap();
            let n = options.max_step_size_per_thread as usize * options.threads;
            while total < n {
                let Some(buffers) = self.inner.buffers.lock().unwrap().pop_front() else {
                    break;
                };
                for mut buffer in buffers {
                    total += buffer.flush_all();
                }
            }
            self.consumer.extended(total, RmpValue::Nil);
        } else {
            self.consumer.extended(0, RmpValue::Nil);
        }
    }
}

impl InputReader for InputGenerator {
    fn request(&self, command: InputReaderCommand) {
        match command.as_nonft().unwrap() {
            NonFtInputReaderCommand::Queue => self.queue(),
            NonFtInputReaderCommand::Transition(state) => {
                self.inner.status[self.table].store(state, Ordering::Release);
                self.inner.unpark();
            }
        }
    }
}

static INNER: Mutex<Weak<Inner>> = Mutex::new(Weak::new());

struct Inner {
    /// Status for each of the tables. We only run if all of them can run.
    status: EnumMap<NexmarkTable, Atomic<PipelineState>>,

    /// Options, which can be set from any of the tables but only from one of them.
    options: OnceLock<NexmarkInputOptions>,

    /// The per-table parsers.
    #[allow(clippy::type_complexity)]
    parsers: Mutex<EnumMap<NexmarkTable, Option<Box<dyn Parser>>>>,

    /// The per-table consumers.
    #[allow(clippy::type_complexity)]
    consumers: Mutex<EnumMap<NexmarkTable, Option<Box<dyn InputConsumer>>>>,

    /// The threads to wake up when we unpark.
    ///
    /// While we're waiting for the connectors for all the tables to be brought
    /// to the running state for the first time, this is only the worker
    /// thread. After that, we start the generator threads and they get
    /// populated here instead.
    threads: Mutex<Vec<Thread>>,

    buffers: Mutex<VecDeque<Vec<Box<dyn InputBuffer>>>>,
}

impl Inner {
    pub fn new() -> Arc<Self> {
        let inner = Arc::new(Self {
            status: EnumMap::from_fn(|_| Atomic::new(PipelineState::Paused)),
            options: OnceLock::new(),
            parsers: Mutex::new(EnumMap::default()),
            consumers: Mutex::new(EnumMap::default()),
            threads: Mutex::new(Vec::new()),
            buffers: Mutex::new(VecDeque::new()),
        });
        thread::Builder::new()
            .name(String::from("nexmark"))
            .spawn({
                let inner = Arc::clone(&inner);
                || inner.worker_thread()
            })
            .unwrap();
        inner
    }
    pub fn merge(
        &self,
        config: &NexmarkInputConfig,
        consumer: Box<dyn InputConsumer>,
        parser: Box<dyn Parser>,
    ) -> AnyResult<()> {
        let mut parsers = self.parsers.lock().unwrap();
        if parsers[config.table].is_some() {
            return Err(anyhow!(
                "more than one Nexmark input connector for {:?}",
                config.table
            ));
        }
        parsers[config.table] = Some(parser);
        drop(parsers);

        self.consumers.lock().unwrap()[config.table] = Some(consumer);

        if let Some(options) = config.options.as_ref() {
            if self.options.set(options.clone()).is_err() {
                return Err(anyhow!(
                    "can't configure Nexmark options from two different connectors"
                ));
            }
        }

        Ok(())
    }

    pub fn unpark(&self) {
        for thread in self.threads.lock().unwrap().iter() {
            thread.unpark();
        }
    }

    /// Returns the pipeline's overall status based on the three underlying connectors:
    ///
    /// - We're terminated if any of the connectors are terminated.
    /// - Otherwise, we're paused if any of them are paused.
    /// - Otherwise, all of them are running, so we are running.
    ///
    /// To be honest, we could probably just use the status of the `bid`
    /// connector, because it's 92% of the records, and ignore the rest.
    fn status(&self) -> PipelineState {
        let mut state = PipelineState::Running;
        for (_table, status) in &self.status {
            let status = status.load(Ordering::Acquire);
            match status {
                PipelineState::Terminated => return PipelineState::Terminated,
                PipelineState::Paused => state = PipelineState::Paused,
                PipelineState::Running => (),
            }
        }
        state
    }

    /// Waits as long as the pipeline is paused, then returns `Ok` if we should
    /// run or `Err` if we should exit.
    fn wait_to_run(&self) -> Result<(), ()> {
        loop {
            match self.status() {
                PipelineState::Paused => thread::park(),
                PipelineState::Running => {
                    return Ok(());
                }
                PipelineState::Terminated => return Err(()),
            }
        }
    }

    /// Returns a CSV writer with our style for `inner`.
    fn make_csv_writer(inner: Vec<u8>) -> CsvWriter<Cursor<Vec<u8>>> {
        CsvWriterBuilder::new()
            .has_headers(false)
            .from_writer(Cursor::new(inner))
    }

    /// Main thread of the Nexmark connector.
    fn worker_thread(self: Arc<Self>) {
        // Wait until we're running.
        *self.threads.lock().unwrap() = vec![thread::current().clone()];
        if self.wait_to_run().is_err() {
            return;
        }

        let parsers =
            mem::take(&mut *self.parsers.lock().unwrap()).map(|_table, parser| parser.unwrap());
        let consumers = mem::take(&mut *self.consumers.lock().unwrap())
            .map(|_table, consumer| consumer.unwrap());

        // Start all the generator threads.
        let options = self.options.get_or_init(Default::default);
        let barrier = Arc::new(Barrier::new(options.threads));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let generators = (0..options.threads)
            .map(|index| {
                let parsers = EnumMap::from_fn(|table| parsers[table].fork());
                let consumer = consumers[NexmarkTable::Bid].clone();
                let queue = queue.clone();
                let barrier = barrier.clone();
                let inner = Arc::clone(&self);
                thread::Builder::new()
                    .name(format!("nexmark-{index}"))
                    .spawn(move || inner.generate_thread(parsers, consumer, queue, index, barrier))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        drop(barrier);
        drop(parsers);

        // Make sure all the generator threads can get unparked, and then unpark
        // them for the first time to avoid a missed wakeup.
        let threads = generators
            .iter()
            .map(|handle| handle.thread().clone())
            .collect::<Vec<_>>();
        *self.threads.lock().unwrap() = threads;
        self.unpark();

        // Let the generator threads run and wait for them to die.
        for handle in generators {
            handle.join().unwrap();
        }

        // Input is exhausted.
        for (_table, consumer) in consumers {
            consumer.eoi();
        }
    }

    #[allow(clippy::type_complexity)]
    fn generate_thread(
        self: Arc<Self>,
        mut parsers: EnumMap<NexmarkTable, Box<dyn Parser>>,
        consumer: Box<dyn InputConsumer>,
        queue: Arc<Mutex<VecDeque<Option<Box<dyn InputBuffer>>>>>,
        index: usize,
        barrier: Arc<Barrier>,
    ) {
        let options = self.options.get().unwrap();

        // Calculate the exact number of times to wait on `barrier`. If we wait
        // any fewer times than that, the other threads will get stuck (if we
        // wait more, we'll get stuck). It's harmless if it's greater than the
        // number of batches.
        let n_batches = options
            .events
            .div_ceil(options.batch_size_per_thread * options.threads as u64);

        let generator_options = GeneratorOptions {
            max_events: options.events,
            num_event_generators: options.threads,
            ..GeneratorOptions::default()
        };
        let mut generator = NexmarkGenerator::new(
            GeneratorConfig::new(generator_options, 0, 0, index),
            ThreadRng::default(),
            0,
        );

        let mut buffers = EnumMap::from_fn(|_| Vec::new());

        for i in 0..n_batches {
            // Wait until we're ready to run.
            if self.wait_to_run().is_err() {
                // Make sure we synchronize exactly `n_batches` times.
                for _ in i..n_batches {
                    barrier.wait();
                }
                return;
            }

            // Compose a batch into the writers.
            let mut writers =
                EnumMap::from_fn(|table| Self::make_csv_writer(mem::take(&mut buffers[table])));
            let mut n = 0;
            for NextEvent { event, .. } in &mut generator {
                match event {
                    Event::Person(person) => {
                        writers[NexmarkTable::Person].serialize(person).unwrap()
                    }
                    Event::Auction(auction) => {
                        writers[NexmarkTable::Auction].serialize(auction).unwrap()
                    }
                    Event::Bid(bid) => writers[NexmarkTable::Bid].serialize(bid).unwrap(),
                }
                n += 1;
                if n >= options.batch_size_per_thread {
                    break;
                }
            }

            // Parse the batch into per-table InputBuffers.
            let buffers = writers
                .into_iter()
                .map(|(table, writer)| {
                    let data = writer.into_inner().unwrap().into_inner();
                    let parser = &mut parsers[table];
                    let (buffer, _errors) = parser.parse(data.as_slice());
                    buffer
                })
                .collect::<Vec<_>>();
            queue.lock().unwrap().extend(buffers.into_iter());

            // Synchronize with the other threads.
            if barrier.wait().is_leader() {
                let buffers = queue
                    .lock()
                    .unwrap()
                    .drain(..options.threads * 3)
                    .flatten()
                    .collect::<Vec<_>>();
                let num_records = buffers.iter().map(|buffer| buffer.len()).sum();
                self.buffers.lock().unwrap().push_back(buffers);
                consumer.buffered(num_records, 0);
            }
        }
    }
}
