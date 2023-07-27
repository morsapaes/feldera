// Browse tables and views & insert data into tables.

import Grid from '@mui/material/Grid'
import { useQuery } from '@tanstack/react-query'
import { useRouter } from 'next/router'
import { SyntheticEvent, useEffect, useState } from 'react'
import PageHeader from 'src/layouts/components/page-header'
import { Pipeline, PipelineId, PipelineRevision, PipelineStatus } from 'src/types/manager'
import { InspectionTable } from 'src/streaming/inspection/InspectionTable'
import {
  Alert,
  AlertTitle,
  Breadcrumbs,
  FormControl,
  InputLabel,
  Link,
  ListSubheader,
  MenuItem,
  Select,
  SelectChangeEvent
} from '@mui/material'
import { Icon } from '@iconify/react'
import { Controller, useForm } from 'react-hook-form'
import { ErrorBoundary } from 'react-error-boundary'
import { ErrorOverlay } from 'src/components/table/ErrorOverlay'
import Tab from '@mui/material/Tab'
import TabList from '@mui/lab/TabList'
import TabPanel from '@mui/lab/TabPanel'
import TabContext from '@mui/lab/TabContext'
import { InsertionTable } from 'src/streaming/import/InsertionTable'

const TitleBreadCrumb = (props: { pipeline: Pipeline; relation: string; tables: string[]; views: string[] }) => {
  const { tables, views } = props
  const { descriptor } = props.pipeline
  const pipeline_id = descriptor.pipeline_id

  const router = useRouter()
  const view = router.query.view

  const switchRelation = (e: SelectChangeEvent<string>) => {
    router.push(`/streaming/inspection/${pipeline_id}/${e.target.value}`)
  }

  interface IFormInputs {
    relation: string
  }

  const { control } = useForm<IFormInputs>({
    defaultValues: {
      relation: view as string
    }
  })

  return typeof view === 'string' && (tables.length > 0 || views.length > 0) ? (
    <Breadcrumbs separator={<Icon icon='bx:chevron-right' fontSize={20} />} aria-label='breadcrumb'>
      <Link href='/streaming/management/'>{descriptor.name}</Link>
      <Controller
        name='relation'
        control={control}
        defaultValue={view}
        render={({ field: { onChange, value } }) => {
          return (
            <FormControl>
              <InputLabel htmlFor='relation-select'>Relation</InputLabel>
              <Select
                label='Select Relation'
                id='relation-select'
                onChange={e => {
                  e.preventDefault()
                  switchRelation(e)
                  onChange(e)
                }}
                value={value}
              >
                <ListSubheader>Tables</ListSubheader>
                {tables.map(item => (
                  <MenuItem key={item} value={item}>
                    {item}
                  </MenuItem>
                ))}
                <ListSubheader>Views</ListSubheader>
                {views.map(item => (
                  <MenuItem key={item} value={item}>
                    {item}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          )
        }}
      />
    </Breadcrumbs>
  ) : (
    <></>
  )
}

const TableWithInsertTab = (props: {
  pipeline: Pipeline
  handleChange: ((event: SyntheticEvent<Element, Event>, value: any) => void) | undefined
  tab: string
  relation: string
}) => {
  const logError = (error: Error) => {
    console.error('InspectionTable error: ', error)
  }

  const { pipeline, handleChange, tab, relation } = props
  return (
    <TabContext value={tab}>
      <TabList centered variant='fullWidth' onChange={handleChange} aria-label='tabs to insert and browse relations'>
        <Tab value='browse' label={`Browse ${relation}`} />
        <Tab value='insert' label='Insert New Rows' />
      </TabList>
      <TabPanel value='browse'>
        <ViewDataTable pipeline={pipeline} relation={relation} />
      </TabPanel>
      <TabPanel value='insert'>
        {pipeline.state.current_status === PipelineStatus.RUNNING ? (
          <ErrorBoundary FallbackComponent={ErrorOverlay} onError={logError} key={location.pathname}>
            <InsertionTable pipeline={pipeline} name={relation} />
          </ErrorBoundary>
        ) : (
          <Alert severity='info'>
            <AlertTitle>Pipeline not running</AlertTitle>
            Start the pipeline to insert data.
          </Alert>
        )}
      </TabPanel>
    </TabContext>
  )
}

const ViewDataTable = (props: { pipeline: Pipeline; relation: string }) => {
  const { pipeline, relation } = props
  const logError = (error: Error) => {
    console.error('InspectionTable error: ', error)
  }

  return pipeline.state.current_status === PipelineStatus.RUNNING ||
    pipeline.state.current_status === PipelineStatus.PAUSED ? (
    <ErrorBoundary FallbackComponent={ErrorOverlay} onError={logError} key={location.pathname}>
      <InspectionTable pipeline={pipeline} name={relation} />
    </ErrorBoundary>
  ) : (
    <ErrorOverlay error={new Error(`'${pipeline.descriptor.name}' is not deployed.`)} />
  )
}

const IntrospectInputOutput = () => {
  const [pipelineId, setPipelineId] = useState<PipelineId | undefined>(undefined)
  const [relation, setRelation] = useState<string | undefined>(undefined)
  const router = useRouter()
  const [tab, setTab] = useState<'browse' | 'insert'>('insert')
  const [tables, setTables] = useState<string[] | undefined>([])
  const [views, setViews] = useState<string[] | undefined>([])

  const handleChange = (event: SyntheticEvent, newValue: 'browse' | 'insert') => {
    setTab(newValue)
  }

  // Parse config, view, tab arguments from router query
  useEffect(() => {
    if (!router.isReady) {
      return
    }
    const { config, view, tab } = router.query
    if (typeof tab === 'string' && (tab == 'browse' || tab == 'insert')) {
      setTab(tab)
    }
    if (typeof config === 'string') {
      setPipelineId(config)
    }
    if (typeof view === 'string') {
      setRelation(view)
    }
  }, [pipelineId, setPipelineId, setRelation, router])

  // Load the pipeline
  const [pipeline, setPipeline] = useState<Pipeline | undefined>(undefined)
  const configQuery = useQuery<Pipeline>(['pipelineStatus', { pipeline_id: pipelineId }], {
    enabled: pipelineId !== undefined
  })
  useEffect(() => {
    if (!configQuery.isLoading && !configQuery.isError) {
      setPipeline(configQuery.data)
    }
  }, [configQuery.isLoading, configQuery.isError, configQuery.data, setPipeline])

  // Load the last revision of the pipeline
  const pipelineRevisionQuery = useQuery<PipelineRevision>(['pipelineLastRevision', { pipeline_id: pipelineId }], {
    enabled: pipelineId !== undefined
  })
  useEffect(() => {
    if (!pipelineRevisionQuery.isLoading && !pipelineRevisionQuery.isError) {
      const pipelineRevision = pipelineRevisionQuery.data
      const program = pipelineRevision?.program
      setTables(program?.schema?.inputs.map(v => v.name) || [])
      setViews(program?.schema?.outputs.map(v => v.name) || [])
    }
  }, [pipelineRevisionQuery.isLoading, pipelineRevisionQuery.isError, pipelineRevisionQuery.data])

  // If we request to be on the insert tab for a view, we force-switch to the
  // browse tab.
  useEffect(() => {
    if (relation && views) {
      if (views.includes(relation) && tab == 'insert') {
        setTab('browse')
      }
    }
  }, [setTab, relation, views, tab])

  return pipelineId !== undefined &&
    !configQuery.isLoading &&
    !configQuery.isError &&
    !pipelineRevisionQuery.isLoading &&
    !pipelineRevisionQuery.isError &&
    pipeline &&
    relation &&
    tables &&
    views &&
    (tables.includes(relation) || views.includes(relation)) ? (
    <Grid container spacing={6} className='match-height'>
      <PageHeader title={<TitleBreadCrumb pipeline={pipeline} relation={relation} tables={tables} views={views} />} />
      <Grid item xs={12}>
        {tables.includes(relation) && (
          <TableWithInsertTab pipeline={pipeline} handleChange={handleChange} tab={tab} relation={relation} />
        )}
        {views.includes(relation) && <ViewDataTable pipeline={pipeline} relation={relation} />}
      </Grid>
    </Grid>
  ) : (
    relation && !((tables && tables.includes(relation)) || (views && views.includes(relation))) && (
      <Alert severity='error'>
        <AlertTitle>Relation not found</AlertTitle>
        Specified unknown table or view: {relation}
      </Alert>
    )
  )
}

export default IntrospectInputOutput
