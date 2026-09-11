use std::{path::Path, sync::Arc};

use anyhow::Context;
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::domain::{
    ChatAttachmentRecord, ComputationalCampaign, ConversationMessage, ExperimentManifest,
    MolecularStructure, ProviderKind, ProviderStatus, QmmmRegionPlan, ResearchProject, RunRecord,
};

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn put_lab_record(&self, value: &crate::laboratory::LabJob) -> anyhow::Result<()> {
        self.put("laboratory_job", &value.id.to_string(), value)
    }
    pub fn put_lab_record_with_message(&self,job:&crate::laboratory::LabJob,message:Option<&ConversationMessage>)->anyhow::Result<()>{
        let mut connection=self.connection.lock();let transaction=connection.transaction()?;
        transaction.execute("INSERT OR REPLACE INTO objects(kind,id,json) VALUES ('laboratory_job',?1,?2)",params![job.id.to_string(),serde_json::to_string(job)?])?;
        if let Some(message)=message{
            let saved:Option<String>=transaction.query_row("SELECT json FROM objects WHERE kind='message' AND id=?1",params![message.id.to_string()],|row|row.get(0)).optional()?;
            if let Some(saved)=saved{let saved:ConversationMessage=serde_json::from_str(&saved)?;anyhow::ensure!(saved.project_id==message.project_id&&saved.role==message.role&&saved.content==message.content&&saved.metadata["laboratory_session_id"]==message.metadata["laboratory_session_id"],"Message identity is already bound to different content or a different conversation");}
            transaction.execute("INSERT OR REPLACE INTO objects(kind,id,json) VALUES ('message',?1,?2)",params![message.id.to_string(),serde_json::to_string(message)?])?;
        }
        transaction.commit()?;Ok(())
    }
    pub fn lab_record(&self, id: Uuid) -> anyhow::Result<Option<crate::laboratory::LabJob>> {
        self.get("laboratory_job", &id.to_string())
    }
    pub fn lab_records(&self) -> anyhow::Result<Vec<crate::laboratory::LabJob>> {
        self.list("laboratory_job", 10000)
    }
    /// Identify the linked database engine for packaged runtime evidence.
    pub fn sqlite_runtime(&self) -> anyhow::Result<serde_json::Value> {
        let (version, source_id): (String, String) = self.connection.lock().query_row(
            "SELECT sqlite_version(), sqlite_source_id()", [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(serde_json::json!({
            "version": version,
            "version_number": rusqlite::version_number(),
            "source_id": source_id,
            "linkage": "bundled",
        }))
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("unable to open database at {}", path.display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS objects (
                kind TEXT NOT NULL,
                id TEXT NOT NULL,
                json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (kind, id)
            );

            CREATE INDEX IF NOT EXISTS ix_objects_kind_updated
                ON objects(kind, updated_at DESC);

            CREATE TABLE IF NOT EXISTS research_asset_payloads (
                id TEXT PRIMARY KEY, project_id TEXT NOT NULL, bytes BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS manifest_revision_counter (
                project_id TEXT PRIMARY KEY, revision INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS provider_settings (
                provider TEXT PRIMARY KEY,
                model TEXT NOT NULL,
                base_url TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn put_agent_task(&self, task: &crate::agent::tasks::ResearchTask) -> anyhow::Result<()> { self.put("agent_task", &task.id.to_string(), task) }
    pub fn put_research_asset(&self, asset: &crate::research::assets::ResearchAsset, bytes: &[u8]) -> anyhow::Result<()> {
        let mut connection=self.connection.lock(); let tx=connection.transaction()?;
        tx.execute("INSERT OR REPLACE INTO research_asset_payloads(id,project_id,bytes) VALUES (?1,?2,?3)",params![asset.id.to_string(),asset.project_id.to_string(),bytes])?;
        tx.execute("INSERT OR REPLACE INTO objects(kind,id,json) VALUES ('research_asset',?1,?2)",params![asset.id.to_string(),serde_json::to_string(asset)?])?;
        tx.commit()?; Ok(())
    }
    pub fn research_assets(&self,id:Uuid)->anyhow::Result<Vec<crate::research::assets::ResearchAsset>> {self.research_rows("research_asset",id)}
    pub fn get_research_asset(&self,id:Uuid)->anyhow::Result<Option<crate::research::assets::ResearchAsset>> {self.get("research_asset",&id.to_string())}
    pub fn research_asset_bytes(&self,id:Uuid)->anyhow::Result<Option<Vec<u8>>> {
        Ok(self.connection.lock().query_row("SELECT bytes FROM research_asset_payloads WHERE id=?1",params![id.to_string()],|row|row.get(0)).optional()?)
    }
    pub fn put_asset_search(&self,v:&crate::research::assets::AssetSearch)->anyhow::Result<()> {self.put("asset_search",&v.id.to_string(),v)}
    pub fn asset_searches(&self,id:Uuid)->anyhow::Result<Vec<crate::research::assets::AssetSearch>> {self.research_rows("asset_search",id)}
    pub fn put_studio_design(&self,id:Uuid,v:&serde_json::Value)->anyhow::Result<()> {self.put("studio_design",&id.to_string(),v)}
    pub fn get_studio_design(&self,id:Uuid)->anyhow::Result<Option<serde_json::Value>> {self.get("studio_design",&id.to_string())}
    pub fn studio_designs(&self,id:Uuid)->anyhow::Result<Vec<serde_json::Value>> {self.research_rows("studio_design",id)}
    pub fn get_agent_task(&self, id: Uuid) -> anyhow::Result<Option<crate::agent::tasks::ResearchTask>> { self.get("agent_task", &id.to_string()) }
    pub fn list_agent_tasks(&self) -> anyhow::Result<Vec<crate::agent::tasks::ResearchTask>> { self.list("agent_task", 10000) }

    pub fn put_research_plan(&self,v:&crate::research::Plan)->anyhow::Result<()> {self.put("research_plan",&v.id.to_string(),v)}
    pub fn put_research_search(&self,v:&crate::research::Search)->anyhow::Result<()> {self.put("research_search",&v.id.to_string(),v)}
    pub fn put_research_task(&self,v:&crate::research::TaskRecord)->anyhow::Result<()> {self.put("research_task",&v.id.to_string(),v)}
    pub fn put_research_data(&self,id:Uuid,v:&serde_json::Value)->anyhow::Result<()> {self.put("research_data",&id.to_string(),v)}
    pub fn research_plans(&self,id:Uuid)->anyhow::Result<Vec<crate::research::Plan>> {self.research_rows("research_plan",id)}
    pub fn research_searches(&self,id:Uuid)->anyhow::Result<Vec<crate::research::Search>> {self.research_rows("research_search",id)}
    pub fn research_tasks(&self,id:Uuid)->anyhow::Result<Vec<crate::research::TaskRecord>> {self.research_rows("research_task",id)}
    pub fn research_data(&self,id:Uuid)->anyhow::Result<Vec<serde_json::Value>> {self.research_rows("research_data",id)}
    fn research_rows<T:DeserializeOwned>(&self,kind:&str,id:Uuid)->anyhow::Result<Vec<T>> {
        let c=self.connection.lock();let mut stmt=c.prepare("SELECT json FROM objects WHERE kind=?1 AND json_extract(json,'$.project_id')=?2 ORDER BY updated_at DESC, rowid DESC")?;
        let rows=stmt.query_map(params![kind,id.to_string()],|r|r.get::<_,String>(0))?;
        let mut out=Vec::new();for row in rows {out.push(serde_json::from_str(&row?)?);}Ok(out)
    }

    fn put<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> anyhow::Result<()> {
        let json = serde_json::to_string(value)?;
        self.connection.lock().execute(
            r#"
            INSERT INTO objects(kind, id, json, updated_at)
            VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
            ON CONFLICT(kind, id) DO UPDATE SET
                json = excluded.json,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![kind, id, json],
        )?;
        Ok(())
    }

    fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> anyhow::Result<Option<T>> {
        let json: Option<String> = self
            .connection
            .lock()
            .query_row(
                "SELECT json FROM objects WHERE kind = ?1 AND id = ?2",
                params![kind, id],
                |row| row.get(0),
            )
            .optional()?;

        json.map(|value| serde_json::from_str(&value).context("stored JSON is invalid"))
            .transpose()
    }

    fn list<T: DeserializeOwned>(&self, kind: &str, limit: usize) -> anyhow::Result<Vec<T>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            "SELECT json FROM objects WHERE kind = ?1 ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![kind, limit as i64], |row| row.get::<_, String>(0))?;

        let mut values = Vec::new();
        for row in rows {
            let json = row?;
            values.push(
                serde_json::from_str(&json)
                    .with_context(|| format!("invalid stored {kind} record"))?,
            );
        }
        Ok(values)
    }

    fn delete(&self, kind: &str, id: &str) -> anyhow::Result<()> {
        self.connection.lock().execute(
            "DELETE FROM objects WHERE kind = ?1 AND id = ?2",
            params![kind, id],
        )?;
        Ok(())
    }

    pub fn experiment_receipt(&self,id:Uuid)->anyhow::Result<Option<serde_json::Value>>{self.get("experiment_receipt",&id.to_string())}
    pub fn put_experiment_receipt(&self,id:Uuid,value:&serde_json::Value)->anyhow::Result<()>{self.put("experiment_receipt",&id.to_string(),value)}

    pub fn put_project(&self, project: &ResearchProject) -> anyhow::Result<()> {
        self.put("project", &project.id.to_string(), project)
    }

    pub fn get_project(&self, id: Uuid) -> anyhow::Result<Option<ResearchProject>> {
        self.get("project", &id.to_string())
    }

    pub fn list_projects(&self) -> anyhow::Result<Vec<ResearchProject>> {
        self.list("project", 2_000)
    }

    pub fn delete_project(&self, project_id: Uuid) -> anyhow::Result<()> {
        if self.list_agent_tasks()?.iter().any(|task| task.project_id == project_id && task.state == crate::agent::tasks::TaskState::Running) {
            anyhow::bail!("pause or cancel the active research session before deleting this project");
        }
        for study in self.list_studies()?.into_iter().filter(|s|s.project_id == project_id) {
            if matches!(study.state.as_str(), "running" | "pausing") {
                anyhow::bail!("pause or end the active discovery campaign before deleting its research world");
            }
        }

        for dossier in self.list_dossiers()?.into_iter().filter(|d|d.project_id==project_id) {
            if matches!(dossier.state.as_str(),"running"|"stopping"){anyhow::bail!("stop verification before deleting the research world");}
        }
        for dossier in self.list_dossiers()?.into_iter().filter(|d|d.project_id==project_id) {
            for search in self.list_literature_searches(dossier.id)?{self.delete("verification_search",&search.id.to_string())?;}
            for review in self.list_verification_reviews(dossier.id)?{self.delete("verification_review",&review.id.to_string())?;}
            self.delete("verification_dossier",&dossier.id.to_string())?;
        }
        for entry in self.list_catalog()?.into_iter().filter(|e|e.project_id==project_id){self.delete("comparison_catalog",&entry.id.to_string())?;}

        for manifest in self.list_manifests(project_id, 10_000)? {
            self.delete("manifest", &manifest.id.to_string())?;
        }
        for message in self.list_messages(project_id, 50_000)? {
            self.delete("message", &message.id.to_string())?;
        }
        for attachment in self
            .list::<ChatAttachmentRecord>("attachment", 50_000)?
            .into_iter()
            .filter(|value| value.project_id == project_id)
        {
            self.delete("attachment", &attachment.id.to_string())?;
        }
        for run in self
            .list::<RunRecord>("run", 50_000)?
            .into_iter()
            .filter(|value| value.project_id == project_id)
        {
            self.delete("run_analysis", &run.id.to_string())?;
            self.delete("run", &run.id.to_string())?;
        }
        for structure in self.list_molecules(Some(project_id), 10_000)? {
            self.delete_molecule(structure.id)?;
        }
        // Research records and studies belong to this world; the usage ledger remains for accounting.
        for study in self.list_studies()?.into_iter().filter(|s|s.project_id == project_id) {
            self.delete("discovery_study", &study.id.to_string())?;
        }
        let connection = self.connection.lock();
        connection.execute("DELETE FROM research_asset_payloads WHERE project_id=?1",params![project_id.to_string()])?;
        connection.execute("DELETE FROM objects WHERE kind IN ('agent_task','research_asset','asset_search','studio_design','research_notebook','notebook_revision','research_plan','research_search','research_task','research_data','experiment_receipt') AND json_extract(json, '$.project_id') = ?1", params![project_id.to_string()])?;
        connection.execute("DELETE FROM manifest_revision_counter WHERE project_id = ?1", params![project_id.to_string()])?;
        drop(connection);
        self.delete("project", &project_id.to_string())
    }

    pub fn put_manifest(&self, manifest: &ExperimentManifest) -> anyhow::Result<()> {
        self.put("manifest", &manifest.id.to_string(), manifest)
    }

    pub fn get_manifest(&self, id: Uuid) -> anyhow::Result<Option<ExperimentManifest>> {
        self.get("manifest", &id.to_string())
    }

    pub fn list_manifests(
        &self,
        project_id: Uuid,
        limit: usize,
    ) -> anyhow::Result<Vec<ExperimentManifest>> {
        let mut manifests = self
            .list::<ExperimentManifest>("manifest", 20_000)?
            .into_iter()
            .filter(|value| value.project_id == project_id)
            .collect::<Vec<_>>();
        manifests.sort_by(|left, right| right.revision.cmp(&left.revision));
        manifests.truncate(limit.clamp(1, 10_000));
        Ok(manifests)
    }

    /// Reserve monotonically increasing revisions across chat and campaign workers.
    /// Gaps after rejected proposals are intentional; duplicate revision numbers are not.
    pub fn next_manifest_revision(&self, project_id: Uuid) -> anyhow::Result<u32> {
        let mut connection = self.connection.lock();
        let tx = connection.transaction()?;
        let project = project_id.to_string();
        let stored: i64 = tx.query_row(
            "SELECT COALESCE(MAX(CAST(json_extract(json, '$.revision') AS INTEGER)), 0) FROM objects WHERE kind = 'manifest' AND json_extract(json, '$.project_id') = ?1",
            params![project], |row| row.get(0),
        )?;
        let reserved: i64 = tx.query_row(
            "SELECT revision FROM manifest_revision_counter WHERE project_id = ?1",
            params![project], |row| row.get(0),
        ).optional()?.unwrap_or(0);
        let next = u32::try_from(stored.max(reserved))?.checked_add(1).context("manifest revision overflow")?;
        tx.execute("INSERT INTO manifest_revision_counter(project_id, revision) VALUES (?1, ?2) ON CONFLICT(project_id) DO UPDATE SET revision = excluded.revision", params![project, i64::from(next)])?;
        tx.commit()?;
        Ok(next)
    }

    pub fn put_message(&self, message: &ConversationMessage) -> anyhow::Result<()> {
        self.put("message", &message.id.to_string(), message)
    }

    pub fn list_messages(
        &self,
        project_id: Uuid,
        limit: usize,
    ) -> anyhow::Result<Vec<ConversationMessage>> {
        let mut messages = self
            .list::<ConversationMessage>("message", 50_000)?
            .into_iter()
            .filter(|value| value.project_id == project_id)
            .collect::<Vec<_>>();
        messages.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        let keep = limit.clamp(1, 20_000);
        if messages.len() > keep {
            messages.drain(0..messages.len() - keep);
        }
        Ok(messages)
    }

    pub fn put_attachment(&self, attachment: &ChatAttachmentRecord) -> anyhow::Result<()> {
        self.put("attachment", &attachment.id.to_string(), attachment)
    }

    pub fn list_attachments_for_message(
        &self,
        message_id: Uuid,
    ) -> anyhow::Result<Vec<ChatAttachmentRecord>> {
        let mut values = self
            .list::<ChatAttachmentRecord>("attachment", 50_000)?
            .into_iter()
            .filter(|value| value.message_id == message_id)
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(values)
    }

    pub fn put_run(&self, run: &RunRecord) -> anyhow::Result<()> {
        self.put("run", &run.id.to_string(), run)
    }

    pub fn put_run_checkpoint(&self, id: Uuid, value: &serde_json::Value) -> anyhow::Result<()> {
        self.put("run_checkpoint", &id.to_string(), value)
    }

    pub fn get_run_checkpoint(&self, id: Uuid) -> anyhow::Result<Option<serde_json::Value>> {
        self.get("run_checkpoint", &id.to_string())
    }

    pub fn clear_run_checkpoint(&self, id: Uuid) -> anyhow::Result<()> {
        self.connection.lock().execute("DELETE FROM objects WHERE kind='run_checkpoint' AND id=?1", params![id.to_string()])?;
        Ok(())
    }

    pub fn put_run_recovery(&self, id: Uuid, value: &serde_json::Value) -> anyhow::Result<()> {
        self.put("run_recovery", &id.to_string(), value)
    }

    pub fn get_run_recovery(&self, id: Uuid) -> anyhow::Result<Option<serde_json::Value>> {
        self.get("run_recovery", &id.to_string())
    }

    pub fn get_run(&self, id: Uuid) -> anyhow::Result<Option<RunRecord>> {
        self.get("run", &id.to_string())
    }

    pub fn list_runs(&self, limit: usize) -> anyhow::Result<Vec<RunRecord>> {
        self.list("run", limit.clamp(1, 50_000))
    }

    pub fn put_molecule(&self, structure: &MolecularStructure) -> anyhow::Result<()> {
        self.put("molecule", &structure.id.to_string(), structure)
    }

    pub fn get_molecule(&self, id: Uuid) -> anyhow::Result<Option<MolecularStructure>> {
        self.get("molecule", &id.to_string())
    }

    pub fn list_molecules(
        &self,
        project_id: Option<Uuid>,
        limit: usize,
    ) -> anyhow::Result<Vec<MolecularStructure>> {
        let mut values = self.list::<MolecularStructure>("molecule", 20_000)?;
        if let Some(project_id) = project_id {
            values.retain(|value| value.project_id == Some(project_id));
        }
        values.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        values.truncate(limit.clamp(1, 10_000));
        Ok(values)
    }

    pub fn delete_molecule(&self, id: Uuid) -> anyhow::Result<()> {
        for plan in self
            .list::<QmmmRegionPlan>("qmmm_plan", 20_000)?
            .into_iter()
            .filter(|value| value.structure_id == id)
        {
            self.delete("qmmm_plan", &plan.id.to_string())?;
        }
        for campaign in self
            .list::<ComputationalCampaign>("campaign", 20_000)?
            .into_iter()
            .filter(|value| value.structure_id == id)
        {
            self.delete("campaign", &campaign.id.to_string())?;
        }
        self.delete("molecule", &id.to_string())
    }

    pub fn put_qmmm_plan(&self, plan: &QmmmRegionPlan) -> anyhow::Result<()> {
        self.put("qmmm_plan", &plan.id.to_string(), plan)
    }

    pub fn get_qmmm_plan(&self, id: Uuid) -> anyhow::Result<Option<QmmmRegionPlan>> {
        self.get("qmmm_plan", &id.to_string())
    }

    pub fn list_qmmm_plans(&self, structure_id: Uuid) -> anyhow::Result<Vec<QmmmRegionPlan>> {
        let mut values = self
            .list::<QmmmRegionPlan>("qmmm_plan", 20_000)?
            .into_iter()
            .filter(|value| value.structure_id == structure_id)
            .collect::<Vec<_>>();
        values.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(values)
    }

    pub fn put_campaign(&self, campaign: &ComputationalCampaign) -> anyhow::Result<()> {
        self.put("campaign", &campaign.id.to_string(), campaign)
    }

    pub fn list_campaigns(&self, structure_id: Uuid) -> anyhow::Result<Vec<ComputationalCampaign>> {
        let mut values = self
            .list::<ComputationalCampaign>("campaign", 20_000)?
            .into_iter()
            .filter(|value| value.structure_id == structure_id)
            .collect::<Vec<_>>();
        values.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(values)
    }

    pub fn put_provider_status(&self, status: &ProviderStatus) -> anyhow::Result<()> {
        self.connection.lock().execute(
            r#"
            INSERT INTO provider_settings(provider, model, base_url, updated_at)
            VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
            ON CONFLICT(provider) DO UPDATE SET
                model = excluded.model,
                base_url = excluded.base_url,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![status.provider.account_name(), status.model, status.base_url],
        )?;
        Ok(())
    }

    pub fn provider_status(
        &self,
        provider: ProviderKind,
        key_configured: bool,
    ) -> anyhow::Result<ProviderStatus> {
        let default_url = match provider {
            ProviderKind::OpenAi => "https://api.openai.com",
            ProviderKind::Anthropic => "https://api.anthropic.com",
        };
        let stored: Option<(String, String)> = self
            .connection
            .lock()
            .query_row(
                "SELECT model, base_url FROM provider_settings WHERE provider = ?1",
                params![provider.account_name()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (model, base_url) = stored.unwrap_or_else(|| (String::new(), default_url.to_owned()));
        let model_configured = !model.trim().is_empty();
        Ok(ProviderStatus {
            provider,
            configured: key_configured && model_configured,
            key_configured,
            model_configured,
            model,
            base_url,
        })
    }

    pub fn delete_provider_status(&self, provider: ProviderKind) -> anyhow::Result<()> {
        self.connection.lock().execute(
            "DELETE FROM provider_settings WHERE provider = ?1",
            params![provider.account_name()],
        )?;
        Ok(())
    }
    pub fn put_usage_record(&self, record: &crate::usage::UsageRecord) -> anyhow::Result<()> {
        self.put("usage_call", &record.id.to_string(), record)
    }
    pub fn get_usage_record(&self, id: Uuid) -> anyhow::Result<Option<crate::usage::UsageRecord>> {
        self.get("usage_call", &id.to_string())
    }
    /// No history truncation: admission totals must account for the complete ledger.
    pub fn list_usage_records(&self) -> anyhow::Result<Vec<crate::usage::UsageRecord>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare("SELECT json FROM objects WHERE kind = 'usage_call'")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut records = Vec::new();
        for row in rows { records.push(serde_json::from_str(&row?)?); }
        Ok(records)
    }
    pub fn usage_settings(&self) -> anyhow::Result<crate::usage::UsageSettings> {
        Ok(self.get::<crate::usage::UsageSettings>("usage_settings", "global")?.unwrap_or_default())
    }
    pub fn put_usage_settings(&self, settings: &crate::usage::UsageSettings) -> anyhow::Result<()> {
        self.put("usage_settings", "global", settings)
    }

    pub fn put_run_analysis(&self, id: Uuid, value: &serde_json::Value) -> anyhow::Result<()> {
        self.put("run_analysis", &id.to_string(), value)
    }
    pub fn get_run_analysis(&self, id: Uuid) -> anyhow::Result<Option<serde_json::Value>> {
        self.get("run_analysis", &id.to_string())
    }

    pub fn put_study(&self, study: &crate::discovery::types::Study) -> anyhow::Result<()> {
        self.put("discovery_study", &study.id.to_string(), study)
    }
    pub fn get_study(&self, id: Uuid) -> anyhow::Result<Option<crate::discovery::types::Study>> {
        self.get("discovery_study", &id.to_string())
    }
    pub fn list_studies(&self) -> anyhow::Result<Vec<crate::discovery::types::Study>> {
        self.list("discovery_study", 10000)
    }
    pub fn get_notebook(&self, id: Uuid) -> anyhow::Result<Option<crate::discovery::types::Notebook>> {
        self.get("research_notebook", &id.to_string())
    }
    pub fn put_notebook_version(&self, value: &crate::discovery::types::Notebook) -> anyhow::Result<()> {
        let mut connection = self.connection.lock();
        let tx = connection.transaction()?;
        let project = value.project_id.to_string();
        let actual: Option<String> = tx.query_row(
            "SELECT json FROM objects WHERE kind = 'research_notebook' AND id = ?1",
            params![project], |row| row.get(0),
        ).optional()?;
        let revision = actual.map(|s| serde_json::from_str::<crate::discovery::types::Notebook>(&s)).transpose()?.map(|n| n.revision).unwrap_or(0);
        if value.revision != revision.checked_add(1).context("notebook revision overflow")? {
            anyhow::bail!("notebook changed in another window; reload before saving");
        }
        let body = serde_json::to_string(value)?;
        // INSERT (not upsert) keeps a revision immutable; both writes commit together.
        tx.execute("INSERT INTO objects(kind, id, json, updated_at) VALUES ('notebook_revision', ?1, ?2, CURRENT_TIMESTAMP)", params![format!("{}:{}",project,value.revision),body])?;
        tx.execute("INSERT INTO objects(kind, id, json, updated_at) VALUES ('research_notebook', ?1, ?2, CURRENT_TIMESTAMP) ON CONFLICT(kind, id) DO UPDATE SET json = excluded.json, updated_at = CURRENT_TIMESTAMP", params![project,body])?;
        tx.commit()?;
        Ok(())
    }

    pub fn put_dossier(&self,v:&crate::assurance::types::Dossier)->anyhow::Result<()>{self.put("verification_dossier",&v.id.to_string(),v)}
    pub fn get_dossier(&self,id:Uuid)->anyhow::Result<Option<crate::assurance::types::Dossier>>{self.get("verification_dossier",&id.to_string())}
    pub fn list_dossiers(&self)->anyhow::Result<Vec<crate::assurance::types::Dossier>>{self.list("verification_dossier",10000)}
    pub fn put_catalog_entry(&self,v:&crate::assurance::types::CatalogEntry)->anyhow::Result<()>{self.put("comparison_catalog",&v.id.to_string(),v)}
    pub fn get_catalog_entry(&self,id:Uuid)->anyhow::Result<Option<crate::assurance::types::CatalogEntry>>{self.get("comparison_catalog",&id.to_string())}
    pub fn list_catalog(&self)->anyhow::Result<Vec<crate::assurance::types::CatalogEntry>>{self.list("comparison_catalog",10000)}
    pub fn put_literature_search(&self,v:&crate::assurance::types::SearchRecord)->anyhow::Result<()>{self.put("verification_search",&v.id.to_string(),v)}
    pub fn list_literature_searches(&self,id:Uuid)->anyhow::Result<Vec<crate::assurance::types::SearchRecord>>{
        let connection=self.connection.lock();
        let mut statement=connection.prepare("SELECT json FROM objects WHERE kind='verification_search' AND json_extract(json, '$.dossier_id')=?1")?;
        let data=statement.query_map(params![id.to_string()],|row|row.get::<_,String>(0))?;
        let mut rows=Vec::<crate::assurance::types::SearchRecord>::new();
        for item in data {rows.push(serde_json::from_str(&item?)?);}
        rows.sort_by_key(|v|v.retrieved_at);Ok(rows)
    }
    pub fn put_verification_review(&self,v:&crate::assurance::types::ReviewRecord)->anyhow::Result<()>{self.put("verification_review",&v.id.to_string(),v)}
    pub fn list_verification_reviews(&self,id:Uuid)->anyhow::Result<Vec<crate::assurance::types::ReviewRecord>>{
        let connection=self.connection.lock();
        let mut statement=connection.prepare("SELECT json FROM objects WHERE kind='verification_review' AND json_extract(json, '$.dossier_id')=?1")?;
        let data=statement.query_map(params![id.to_string()],|row|row.get::<_,String>(0))?;
        let mut rows=Vec::<crate::assurance::types::ReviewRecord>::new();
        for item in data {rows.push(serde_json::from_str(&item?)?);}
        rows.sort_by_key(|v|v.recorded_at);Ok(rows)
    }

}

#[cfg(test)]
mod sqlite_runtime_tests {
    use super::*;

    #[test]
    fn linked_sqlite_matches_the_reviewed_security_baseline() {
        let db = Database::open(Path::new(":memory:")).unwrap();
        let runtime = db.sqlite_runtime().unwrap();
        // Changing this pin requires reviewing the new bundled source and SBOM.
        assert_eq!(runtime["version"], "3.53.2");
        assert_eq!(runtime["version_number"], 3_053_002);
        assert_eq!(runtime["source_id"], "2026-06-03 19:12:13 d6e03d8c777cfa2d35e3b60d8ec3e0187f3e9f99d8e2ee9cac695fd6fcdf1a24");
        assert_eq!(runtime["version"], rusqlite::version());
        println!("{}", runtime);
    }

    #[test]
    fn reopening_legacy_database_preserves_records_and_transaction_rollback() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let project_id = Uuid::new_v4();
        let original = serde_json::json!({"id": project_id, "title": "Legacy research", "notes": "DNA α + orbit 🪐"});
        // The pre-upgrade objects/provider schema remains readable without export/import.
        {
            let legacy = Connection::open(&path).unwrap();
            legacy.execute_batch("CREATE TABLE objects(kind TEXT NOT NULL,id TEXT NOT NULL,json TEXT NOT NULL,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,PRIMARY KEY(kind,id)); CREATE TABLE provider_settings(provider TEXT PRIMARY KEY,model TEXT NOT NULL,base_url TEXT NOT NULL,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);").unwrap();
            legacy.execute("INSERT INTO objects(kind,id,json) VALUES ('research_project',?1,?2)", params![project_id.to_string(), original.to_string()]).unwrap();
            legacy.execute("INSERT INTO provider_settings(provider,model,base_url) VALUES (?1,'saved-model','https://api.openai.com')", params![ProviderKind::OpenAi.account_name()]).unwrap();
        }
        {
            let db = Database::open(&path).unwrap();
            let stored: serde_json::Value = db.get("research_project", &project_id.to_string()).unwrap().unwrap();
            assert_eq!(stored, original);
            assert_eq!(db.provider_status(ProviderKind::OpenAi, true).unwrap().model, "saved-model");
            let mut connection = db.connection.lock();
            assert_eq!(connection.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0)).unwrap(), "wal");
            assert_eq!(connection.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
            let transaction = connection.transaction().unwrap();
            transaction.execute("UPDATE objects SET json='{}' WHERE id=?1", params![project_id.to_string()]).unwrap();
            transaction.rollback().unwrap();
        }
        let reopened = Database::open(&path).unwrap();
        assert_eq!(reopened.get::<serde_json::Value>("research_project", &project_id.to_string()).unwrap().unwrap(), original);
        assert_eq!(reopened.connection.lock().query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).unwrap(), "ok");
    }
}
