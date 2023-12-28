use super::{
    ApiKeyDescr, ApiPermission, AttachedConnector, ConnectorDescr, ConnectorId, DBError, Pipeline,
    PipelineDescr, PipelineId, PipelineRevision, PipelineRuntimeState, PipelineStatus,
    ProgramDescr, ProgramId, ProgramSchema, Revision, Version,
};
use crate::api::ProgramStatus;
use crate::auth::TenantId;
use crate::db::{ServiceDescr, ServiceId};
use async_trait::async_trait;
use pipeline_types::config::{ConnectorConfig, RuntimeConfig, ServiceConfig};
use uuid::Uuid;

/// The storage trait contains the methods to interact with the pipeline manager
/// storage layer (e.g., PostgresDB) to implement the public API.
///
/// We use a trait so we can mock the storage layer in tests.
#[async_trait]
pub(crate) trait Storage {
    async fn list_programs(
        &self,
        tenant_id: TenantId,
        with_code: bool,
    ) -> Result<Vec<ProgramDescr>, DBError>;

    /// Update program schema.
    ///
    /// # Note
    /// This should be called after the SQL compilation succeeded, e.g., in the
    /// same transaction that sets status to  [`ProgramStatus::CompilingRust`].
    async fn set_program_schema(
        &self,
        tenant_id: TenantId,
        program_id: ProgramId,
        schema: ProgramSchema,
    ) -> Result<(), DBError> {
        self.update_program(
            tenant_id,
            program_id,
            &None,
            &None,
            &None,
            &None,
            &Some(schema),
            None,
        )
        .await?;
        Ok(())
    }

    /// Update program status after a version check.
    ///
    /// Updates program status to `status` if the current program version in the
    /// database matches `expected_version`. Setting the status to
    /// `ProgramStatus::Pending` resets the schema and is used to queue the
    /// program for compilation.
    async fn set_program_status_guarded(
        &self,
        tenant_id: TenantId,
        program_id: ProgramId,
        expected_version: Version,
        status: ProgramStatus,
    ) -> Result<(), DBError> {
        self.update_program(
            tenant_id,
            program_id,
            &None,
            &None,
            &None,
            &Some(status),
            &None,
            Some(expected_version),
        )
        .await?;
        Ok(())
    }

    /// Create a new program.
    async fn new_program(
        &self,
        tenant_id: TenantId,
        id: Uuid,
        program_name: &str,
        program_description: &str,
        program_code: &str,
    ) -> Result<(ProgramId, Version), DBError>;

    /// Update program name, description and, optionally, code.
    /// XXX: Description should be optional too
    #[allow(clippy::too_many_arguments)]
    async fn update_program(
        &self,
        tenant_id: TenantId,
        program_id: ProgramId,
        program_name: &Option<String>,
        program_description: &Option<String>,
        program_code: &Option<String>,
        status: &Option<ProgramStatus>,
        schema: &Option<ProgramSchema>,
        guard: Option<Version>,
    ) -> Result<Version, DBError>;

    /// Retrieve program descriptor.
    ///
    /// Returns `None` if `program_id` is not found in the database.
    async fn get_program_by_id(
        &self,
        tenant_id: TenantId,
        program_id: ProgramId,
        with_code: bool,
    ) -> Result<ProgramDescr, DBError>;

    /// Lookup program by name.
    async fn get_program_by_name(
        &self,
        tenant_id: TenantId,
        program_name: &str,
        with_code: bool,
    ) -> Result<ProgramDescr, DBError>;

    /// Delete program from the database.
    ///
    /// This will delete all program configs and pipelines.
    async fn delete_program(
        &self,
        tenant_id: TenantId,
        program_id: ProgramId,
    ) -> Result<(), DBError>;

    /// Retrieves all programs in the DB. Intended to be used by
    /// reconciliation loops.
    async fn all_programs(&self) -> Result<Vec<(TenantId, ProgramDescr)>, DBError>;

    /// Retrieves all pipelines in the DB. Intended to be used by
    /// reconciliation loops.
    async fn all_pipelines(&self) -> Result<Vec<(TenantId, PipelineId)>, DBError>;

    /// Retrieves the first pending program from the queue.
    ///
    /// Returns a pending program with the most recent `status_since` or `None`
    /// if there are no pending programs in the DB.
    async fn next_job(&self) -> Result<Option<(TenantId, ProgramId, Version)>, DBError>;

    /// Version the configuration for a pipeline.
    ///
    /// Returns the revision number for that snapshot.
    async fn create_pipeline_revision(
        &self,
        new_revision_id: Uuid,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
    ) -> Result<Revision, DBError>;

    /// Retrieves the current revision for a pipeline (including all immutable
    /// state needed to run it).
    async fn get_last_committed_pipeline_revision(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
    ) -> Result<PipelineRevision, DBError>;

    /// Create a new config.
    #[allow(clippy::too_many_arguments)]
    async fn new_pipeline(
        &self,
        tenant_id: TenantId,
        id: Uuid,
        program_name: &Option<String>,
        pipline_name: &str,
        pipeline_description: &str,
        config: &RuntimeConfig,
        connectors: &Option<Vec<AttachedConnector>>,
    ) -> Result<(PipelineId, Version), DBError>;

    /// Update existing config.
    ///
    /// Update config name and, optionally, YAML.
    #[allow(clippy::too_many_arguments)]
    async fn update_pipeline(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
        program_id: &Option<String>,
        pipline_name: &str,
        pipeline_description: &str,
        config: &Option<RuntimeConfig>,
        connectors: &Option<Vec<AttachedConnector>>,
    ) -> Result<Version, DBError>;

    /// Delete config.
    async fn delete_config(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
    ) -> Result<(), DBError>;

    /// Get input/output status for an attached connector.
    async fn attached_connector_is_input(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
        name: &str,
    ) -> Result<bool, DBError>;

    /// Delete `pipeline` from the DB.
    async fn delete_pipeline(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
    ) -> Result<bool, DBError>;

    /// Retrieve pipeline for a given id.
    async fn get_pipeline_descr_by_id(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
    ) -> Result<PipelineDescr, DBError>;

    async fn get_pipeline_by_id(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
    ) -> Result<Pipeline, DBError>;

    /// Retrieve pipeline for a given name.
    async fn get_pipeline_descr_by_name(
        &self,
        tenant_id: TenantId,
        name: String,
    ) -> Result<PipelineDescr, DBError>;

    async fn get_pipeline_by_name(
        &self,
        tenant_id: TenantId,
        name: String,
    ) -> Result<Pipeline, DBError>;

    async fn get_pipeline_runtime_state(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
    ) -> Result<PipelineRuntimeState, DBError>;

    async fn update_pipeline_runtime_state(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
        state: &PipelineRuntimeState,
    ) -> Result<(), DBError>;

    async fn set_pipeline_desired_status(
        &self,
        tenant_id: TenantId,
        pipeline_id: PipelineId,
        desired_status: PipelineStatus,
    ) -> Result<(), DBError>;

    async fn list_pipelines(&self, tenant_id: TenantId) -> Result<Vec<Pipeline>, DBError>;

    /// Create a new connector.
    async fn new_connector(
        &self,
        tenant_id: TenantId,
        id: Uuid,
        name: &str,
        description: &str,
        config: &ConnectorConfig,
    ) -> Result<ConnectorId, DBError>;

    /// Retrieve connectors list from the DB.
    async fn list_connectors(&self, tenant_id: TenantId) -> Result<Vec<ConnectorDescr>, DBError>;

    /// Retrieve connector descriptor for the given `connector_id`.
    async fn get_connector_by_id(
        &self,
        tenant_id: TenantId,
        connector_id: ConnectorId,
    ) -> Result<ConnectorDescr, DBError>;

    /// Retrieve connector descriptor for the given `name`.
    async fn get_connector_by_name(
        &self,
        tenant_id: TenantId,
        name: String,
    ) -> Result<ConnectorDescr, DBError>;

    /// Update existing connector config.
    ///
    /// Update connector name and, optionally, YAML.
    async fn update_connector(
        &self,
        tenant_id: TenantId,
        connector_id: ConnectorId,
        connector_name: &str,
        description: &str,
        config: &Option<ConnectorConfig>,
    ) -> Result<(), DBError>;

    /// Delete connector from the database.
    ///
    /// This will delete all connector configs and pipelines.
    async fn delete_connector(
        &self,
        tenant_id: TenantId,
        connector_id: ConnectorId,
    ) -> Result<(), DBError>;

    /// Get a list of API key names
    async fn list_api_keys(&self, tenant_id: TenantId) -> Result<Vec<ApiKeyDescr>, DBError>;

    /// Get an API key by name
    async fn get_api_key(&self, tenant_id: TenantId, name: &str) -> Result<ApiKeyDescr, DBError>;

    /// Delete an API key by name
    async fn delete_api_key(&self, tenant_id: TenantId, name: &str) -> Result<(), DBError>;

    /// Persist an SHA-256 hash of an API key in the database
    async fn store_api_key_hash(
        &self,
        tenant_id: TenantId,
        id: Uuid,
        name: &str,
        key: &str,
        permissions: Vec<ApiPermission>,
    ) -> Result<(), DBError>;

    /// Validate an API key against the database by comparing its SHA-256 hash
    /// against the stored value.
    async fn validate_api_key(&self, key: &str) -> Result<(TenantId, Vec<ApiPermission>), DBError>;

    /// Get the tenant ID from the database for a given tenant name and
    /// provider, else create a new tenant ID
    async fn get_or_create_tenant_id(
        &self,
        tenant_name: String,
        provider: String,
    ) -> Result<TenantId, DBError>;

    /// Create a new tenant ID for a given tenant name and provider
    async fn create_tenant_if_not_exists(
        &self,
        tenant_id: Uuid,
        tenant_name: String,
        provider: String,
    ) -> Result<TenantId, DBError>;

    /// Record a URL pointing to a compile. binary Supported URL types are
    /// determined by the compiler service (e.g. file:///)
    async fn create_compiled_binary_ref(
        &self,
        program_id: ProgramId,
        version: Version,
        url: String,
    ) -> Result<(), DBError>;

    /// Retrieve a compiled binary's URL
    async fn get_compiled_binary_ref(
        &self,
        program_id: ProgramId,
        version: Version,
    ) -> Result<Option<String>, DBError>;

    /// Retrieve a compiled binary's URL
    async fn delete_compiled_binary_ref(
        &self,
        program_id: ProgramId,
        version: Version,
    ) -> Result<(), DBError>;

    /// Creates a new service.
    async fn new_service(
        &self,
        tenant_id: TenantId,
        id: Uuid,
        name: &str,
        description: &str,
        config: &ServiceConfig,
    ) -> Result<ServiceId, DBError>;

    /// Retrieves a list of all services of a tenant.
    async fn list_services(&self, tenant_id: TenantId) -> Result<Vec<ServiceDescr>, DBError>;

    /// Retrieves service descriptor for the given
    /// `service_id`.
    async fn get_service_by_id(
        &self,
        tenant_id: TenantId,
        service_id: ServiceId,
    ) -> Result<ServiceDescr, DBError>;

    /// Retrieves service descriptor for the given unique `name`.
    async fn get_service_by_name(
        &self,
        tenant_id: TenantId,
        name: String,
    ) -> Result<ServiceDescr, DBError>;

    /// Updates existing service.
    /// Only the description and config can be updated, as the name is
    /// immutable. The description must be provided and will always be updated.
    /// The config is optional and not updated if not provided.
    async fn update_service(
        &self,
        tenant_id: TenantId,
        service_id: ServiceId,
        description: &str,
        config: &Option<ServiceConfig>,
    ) -> Result<(), DBError>;

    /// Deletes by id the service from the database.
    /// TODO: what are pre-conditions for successful deletion?
    /// TODO: what are post-conditions after successful deletion
    ///       (e.g., cascading)?
    async fn delete_service(
        &self,
        tenant_id: TenantId,
        service_id: ServiceId,
    ) -> Result<(), DBError>;

    /// Check connectivity to the DB
    async fn check_connection(&self) -> Result<(), DBError>;
}
