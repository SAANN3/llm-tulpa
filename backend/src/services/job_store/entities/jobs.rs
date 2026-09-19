use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "jobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub chat_id: i64,
    /// The command line exactly as the model wrote it — not the version with the
    /// package-manager preamble spliced in.
    pub command: String,
    pub workdir: Option<String>,
    /// Where the job's combined stdout+stderr is being written — empty only for the
    /// instant between the row being inserted and its log file being created.
    pub log_path: String,
    pub pid: Option<i64>,
    /// One of `running`/`exited`/`killed`/`lost` — see `JobStatus`, the only thing that
    /// reads or writes this as anything but a bare string.
    pub status: String,
    pub exit_code: Option<i32>,
    pub started_at: DateTimeUtc,
    pub finished_at: Option<DateTimeUtc>,
    /// Whether the chat's model has been told how this job ended — set the moment it
    /// is told (a `notice` message) or has seen the outcome itself (a tool result
    /// showing it), so it's never reported twice.
    pub notified: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
