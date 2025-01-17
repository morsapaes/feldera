//! Feldera Pipeline Manager provides an HTTP API to catalog, compile, and
//! execute SQL programs.
//!
//! # Architecture
//!
//! * Project database.  Programs (including SQL source code), configs, and
//!   pipelines are stored in a Postgres database.  The database is the only
//!   state that is expected to survive across server restarts.  Intermediate
//!   artifacts stored in the file system (see below) can be safely deleted.
//!
//! * Compiler.  The compiler generates a binary crate for each program and adds
//!   it to a cargo workspace that also includes libraries that come with the
//!   SQL libraries.  This way, all precompiled dependencies of the main crate
//!   are reused across programs, thus speeding up compilation.
//!
//! * Runner.  The runner component is responsible for starting and killing
//!   compiled pipelines and for interacting with them at runtime.

mod api_key;
mod config_api;
mod examples;
mod http_io;
mod metrics;
mod pipeline;

use crate::auth::JwkCache;
use crate::config::ApiServerConfig;
use crate::db::storage_postgres::StoragePostgres;
use crate::demo::{read_demos_from_directories, Demo};
use crate::error::ManagerError;
use crate::probe::Probe;
use crate::runner::interaction::RunnerInteraction;
use actix_http::body::BoxBody;
use actix_http::StatusCode;
use actix_web::body::MessageBody;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::http::Method;
use actix_web::Scope;
use actix_web::{
    get,
    web::Data as WebData,
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer,
};
use actix_web_httpauth::middleware::HttpAuthentication;
use actix_web_static_files::ResourceFiles;
use anyhow::{Error as AnyError, Result as AnyResult};
use futures_util::FutureExt;
use log::{error, log, trace, Level};
use std::time::Duration;
use std::{env, net::TcpListener, sync::Arc};
use termbg::{theme, Theme};
use tokio::sync::Mutex;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(
        title = "Feldera API",
        description = r"
With Feldera, users create data pipelines out of SQL programs.
A SQL program comprises tables and views, and includes as well the definition of
input and output connectors for each respectively. A connector defines a data
source or data sink to feed input data into tables or receive output data
computed by the views respectively.

## Pipeline

The API is centered around the **pipeline**, which most importantly consists
out of the SQL program, but also has accompanying metadata and configuration parameters
(e.g., compilation profile, number of workers, etc.).

* A pipeline is identified and referred to by a user-provided unique name.
* The pipeline program is asynchronously compiled when the pipeline is first created or
  its program code or configuration is updated.
* Running the pipeline (*deployment*) is only possible once the program is compiled
* A pipeline cannot be updated while it is running

## Concurrency

Both the pipeline and its program have an associated *version*.
A version is a monotonically increasing number.
Anytime the core fields (name, description, runtime_config, program_code, program_config) are modified,
the pipeline version is incremented.
Anytime the program core fields (program_code, program_config) are modified,
the program version is incremented.
The program version is used internally by the compiler to know when to recompile."
    ),
    paths(
        // Regular pipeline endpoints
        pipeline::list_pipelines,
        pipeline::get_pipeline,
        pipeline::post_pipeline,
        pipeline::put_pipeline,
        pipeline::patch_pipeline,
        pipeline::delete_pipeline,

        // Special pipeline endpoints
        pipeline::post_pipeline_action,
        pipeline::input_endpoint_action,
        pipeline::get_pipeline_logs,
        pipeline::get_pipeline_stats,
        pipeline::get_pipeline_circuit_profile,
        pipeline::get_pipeline_heap_profile,
        pipeline::pipeline_adhoc_sql,
        pipeline::checkpoint_pipeline,

        // HTTP input/output
        http_io::http_input,
        http_io::http_output,

        // API keys
        api_key::list_api_keys,
        api_key::get_api_key,
        api_key::post_api_key,
        api_key::delete_api_key,

        // Configuration
        config_api::get_config_authentication,
        config_api::get_config_demos,
        config_api::get_config,

        // Metrics
        metrics::get_metrics,
    ),
    components(schemas(
        // Authentication
        crate::auth::AuthProvider,
        crate::auth::ProviderAwsCognito,
        crate::auth::ProviderGoogleIdentity,

        // Common
        crate::db::types::common::Version,
        crate::api::config_api::Configuration,

        // Pipeline
        crate::db::types::pipeline::PipelineId,
        crate::db::types::pipeline::PipelineDescr,
        crate::db::types::pipeline::ExtendedPipelineDescr,
        crate::db::types::pipeline::PipelineStatus,
        crate::db::types::pipeline::PipelineDesiredStatus,
        crate::api::pipeline::ListPipelinesQueryParameters,
        crate::api::pipeline::PatchPipeline,
        crate::api::pipeline::ExtendedPipelineDescrOptionalCode,

        // Demo
        crate::demo::Demo,

        // Program
        crate::db::types::program::CompilationProfile,
        crate::db::types::program::SqlCompilerMessage,
        crate::db::types::program::ProgramStatus,
        crate::db::types::program::ProgramConfig,
        crate::db::types::program::ProgramInfo,

        // API key
        crate::db::types::api_key::ApiKeyId,
        crate::db::types::api_key::ApiPermission,
        crate::db::types::api_key::ApiKeyDescr,
        crate::api::api_key::NewApiKeyRequest,
        crate::api::api_key::NewApiKeyResponse,

        // From the feldera-types crate
        feldera_types::config::PipelineConfig,
        feldera_types::config::StorageConfig,
        feldera_types::config::StorageCacheConfig,
        feldera_types::config::RuntimeConfig,
        feldera_types::config::FtConfig,
        feldera_types::config::InputEndpointConfig,
        feldera_types::config::ConnectorConfig,
        feldera_types::config::OutputBufferConfig,
        feldera_types::config::OutputEndpointConfig,
        feldera_types::config::TransportConfig,
        feldera_types::config::FormatConfig,
        feldera_types::config::ResourceConfig,
        feldera_types::transport::adhoc::AdHocInputConfig,
        feldera_types::transport::file::FileInputConfig,
        feldera_types::transport::file::FileOutputConfig,
        feldera_types::transport::http::HttpInputConfig,
        feldera_types::transport::url::UrlInputConfig,
        feldera_types::transport::kafka::KafkaHeader,
        feldera_types::transport::kafka::KafkaHeaderValue,
        feldera_types::transport::kafka::KafkaLogLevel,
        feldera_types::transport::kafka::KafkaInputConfig,
        feldera_types::transport::kafka::KafkaOutputConfig,
        feldera_types::transport::kafka::KafkaOutputFtConfig,
        feldera_types::transport::pubsub::PubSubInputConfig,
        feldera_types::transport::s3::S3InputConfig,
        feldera_types::transport::datagen::DatagenStrategy,
        feldera_types::transport::datagen::RngFieldSettings,
        feldera_types::transport::datagen::GenerationPlan,
        feldera_types::transport::datagen::DatagenInputConfig,
        feldera_types::transport::nexmark::NexmarkInputConfig,
        feldera_types::transport::nexmark::NexmarkTable,
        feldera_types::transport::nexmark::NexmarkInputOptions,
        feldera_types::transport::delta_table::DeltaTableIngestMode,
        feldera_types::transport::delta_table::DeltaTableWriteMode,
        feldera_types::transport::delta_table::DeltaTableReaderConfig,
        feldera_types::transport::delta_table::DeltaTableWriterConfig,
        feldera_types::transport::http::Chunk,
        feldera_types::query::AdhocQueryArgs,
        feldera_types::query::AdHocResultFormat,
        feldera_types::format::json::JsonUpdateFormat,
        feldera_types::program_schema::ProgramSchema,
        feldera_types::program_schema::Relation,
        feldera_types::program_schema::SqlType,
        feldera_types::program_schema::Field,
        feldera_types::program_schema::ColumnType,
        feldera_types::program_schema::IntervalUnit,
        feldera_types::program_schema::SourcePosition,
        feldera_types::program_schema::PropertyValue,
        feldera_types::program_schema::SqlIdentifier,
        feldera_types::error::ErrorResponse,

        // Configuration
        feldera_types::config::OutputBufferConfig,
        feldera_types::config::OutputEndpointConfig,
    ),),
    tags(
        (name = "Pipelines", description = "Manage pipelines and their deployment."),
        (name = "HTTP input/output", description = "Interact with running pipelines using HTTP."),
        (name = "Authentication", description = "Retrieve authentication configuration."),
        (name = "Configuration", description = "Retrieve general configuration."),
        (name = "API keys", description = "Manage API keys."),
        (name = "Metrics", description = "Retrieve pipeline metrics."),
    ),
)]
pub struct ApiDoc;

// `static_files` magic.
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

// The scope for all unauthenticated API endpoints
fn public_scope() -> Scope {
    let openapi = ApiDoc::openapi();

    // Leave this as an empty prefix to load the UI by default. When constructing an
    // app, always attach other scopes without empty prefixes before this one,
    // or route resolution does not work correctly.
    web::scope("")
        .service(config_api::get_config_authentication)
        .service(config_api::get_config)
        .service(SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-doc/openapi.json", openapi))
        .service(healthz)
        .service(ResourceFiles::new("/", generate()).resolve_not_found_to_root())
}

// The scope for all authenticated API endpoints
fn api_scope() -> Scope {
    // Make APIs available under the /v0/ prefix
    web::scope("/v0")
        // Typical pipeline endpoints
        .service(pipeline::list_pipelines)
        .service(pipeline::get_pipeline)
        .service(pipeline::post_pipeline)
        .service(pipeline::put_pipeline)
        .service(pipeline::patch_pipeline)
        .service(pipeline::delete_pipeline)
        // Special pipeline endpoints
        .service(pipeline::post_pipeline_action)
        .service(pipeline::input_endpoint_action)
        .service(pipeline::get_pipeline_logs)
        .service(pipeline::get_pipeline_stats)
        .service(pipeline::get_pipeline_circuit_profile)
        .service(pipeline::get_pipeline_heap_profile)
        .service(pipeline::pipeline_adhoc_sql)
        .service(pipeline::checkpoint_pipeline)
        // API keys endpoints
        .service(api_key::list_api_keys)
        .service(api_key::get_api_key)
        .service(api_key::post_api_key)
        .service(api_key::delete_api_key)
        // HTTP input/output endpoints
        .service(http_io::http_input)
        .service(http_io::http_output)
        // Configuration endpoints
        .service(config_api::get_config_authentication)
        .service(config_api::get_config_demos)
        // Metrics of all pipelines belonging to this tenant
        .service(metrics::get_metrics)
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "JSON web token (JWT) or API key",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .description(Some(
                            r#"Use a JWT token obtained via an OAuth2/OIDC
                               login workflow or an API key obtained via
                               the `/v0/api-keys` endpoint."#,
                        ))
                        .build(),
                ),
            )
        }
    }
}

pub(crate) fn parse_string_param(
    req: &HttpRequest,
    param_name: &'static str,
) -> Result<String, ManagerError> {
    match req.match_info().get(param_name) {
        None => Err(ManagerError::MissingUrlEncodedParam { param: param_name }),
        Some(id) => match id.parse::<String>() {
            Err(e) => Err(ManagerError::InvalidNameParam {
                value: id.to_string(),
                error: e.to_string(),
            }),
            Ok(id) => Ok(id),
        },
    }
}

// The below types and methods are used for running the api-server

pub(crate) struct ServerState {
    // The server must avoid holding this lock for a long time to avoid blocking concurrent
    // requests.
    pub db: Arc<Mutex<StoragePostgres>>,
    runner: RunnerInteraction,
    _config: ApiServerConfig,
    pub jwk_cache: Arc<Mutex<JwkCache>>,
    probe: Arc<Mutex<Probe>>,
    demos: Vec<Demo>,
}

impl ServerState {
    pub async fn new(config: ApiServerConfig, db: Arc<Mutex<StoragePostgres>>) -> AnyResult<Self> {
        let runner = RunnerInteraction::new(config.clone(), db.clone());
        let db_copy = db.clone();
        let demos = read_demos_from_directories(&config.demos_dir);
        Ok(Self {
            db,
            runner,
            _config: config,
            jwk_cache: Arc::new(Mutex::new(JwkCache::new())),
            probe: Probe::new(db_copy).await,
            demos,
        })
    }
}

fn create_listener(api_config: &ApiServerConfig) -> AnyResult<TcpListener> {
    // Check that the port is available before turning into a daemon, so we can fail
    // early if the port is taken.
    let listener =
        TcpListener::bind((api_config.bind_address.clone(), api_config.port)).map_err(|e| {
            AnyError::msg(format!(
                "failed to bind port '{}:{}': {e}",
                &api_config.bind_address, api_config.port
            ))
        })?;
    Ok(listener)
}

/// Logs the responses of the web server.
pub fn log_response(
    res: Result<ServiceResponse<BoxBody>, actix_web::Error>,
) -> Result<ServiceResponse<BoxBody>, actix_web::Error> {
    match &res {
        Ok(response) => {
            let req = response.request();
            let level = if response.status().is_success()
                || response.status() == StatusCode::NOT_MODIFIED
            {
                if req.method() == Method::GET && req.path() == "/healthz" {
                    Level::Trace
                } else {
                    Level::Debug
                }
            } else if response.status().is_client_error() {
                Level::Info
            } else {
                Level::Error
            };
            log!(
                level,
                "Response: {} (size: {:?}) to request {} {}",
                response.status(),
                response.response().body().size(),
                req.method(),
                req.path()
            );
        }
        Err(e) => {
            error!("Service response error: {e}");
        }
    }
    res
}

pub async fn run(db: Arc<Mutex<StoragePostgres>>, api_config: ApiServerConfig) -> AnyResult<()> {
    let listener = create_listener(&api_config)?;
    let state = WebData::new(ServerState::new(api_config.clone(), db).await?);
    let bind_address = api_config.bind_address.clone();
    let port = api_config.port;
    let auth_configuration = match api_config.auth_provider {
        crate::config::AuthProviderType::None => None,
        crate::config::AuthProviderType::AwsCognito => Some(crate::auth::aws_auth_config()),
        crate::config::AuthProviderType::GoogleIdentity => Some(crate::auth::google_auth_config()),
    };
    let server = match auth_configuration {
        // We instantiate an awc::Client that can be used if the api-server needs to
        // make outgoing calls. This object is not meant to have more than one instance
        // per thread (otherwise, it causes high resource pressure on both CPU and fds).
        Some(auth_configuration) => {
            let server = HttpServer::new(move || {
                let auth_middleware = HttpAuthentication::with_fn(crate::auth::auth_validator);
                let client = WebData::new(awc::Client::new());
                App::new()
                    .app_data(state.clone())
                    .app_data(auth_configuration.clone())
                    .app_data(client)
                    .wrap_fn(|req, srv| {
                        trace!("Request: {} {}", req.method(), req.path());
                        srv.call(req).map(log_response)
                    })
                    .wrap(api_config.cors())
                    .service(api_scope().wrap(auth_middleware))
                    .service(public_scope())
            });
            server.listen(listener)?.run()
        }
        None => {
            let server = HttpServer::new(move || {
                let client = WebData::new(awc::Client::new());
                App::new()
                    .app_data(state.clone())
                    .app_data(client)
                    .wrap_fn(|req, srv| {
                        trace!("Request: {} {}", req.method(), req.path());
                        srv.call(req).map(log_response)
                    })
                    .wrap(api_config.cors())
                    .service(api_scope().wrap_fn(|req, srv| {
                        let req = crate::auth::tag_with_default_tenant_id(req);
                        srv.call(req)
                    }))
                    .service(public_scope())
            });
            server.listen(listener)?.run()
        }
    };

    let banner = if theme(Duration::from_millis(500)).unwrap_or(Theme::Light) == Theme::Dark {
        include_str!("../../light-banner.ascii")
    } else {
        include_str!("../../dark-banner.ascii")
    };
    let addr = env::var("BANNER_ADDR").unwrap_or(bind_address);
    let url = format!("http://{}:{}", addr, port);

    println!(
        r"

{banner}

Web console URL: {}
API server URL: {}
Documentation: https://docs.feldera.com/
Version: {}
        ",
        url,
        url,
        env!("CARGO_PKG_VERSION")
    );
    server.await?;
    Ok(())
}

/// This is an internal endpoint and as such is not exposed via OpenAPI
#[get("/healthz")]
async fn healthz(state: WebData<ServerState>) -> Result<HttpResponse, ManagerError> {
    let probe = state.probe.lock().await;
    probe.status_as_http_response()
}
