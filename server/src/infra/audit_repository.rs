//! Audit log repository — PostgreSQL implementation

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::audit::{AuditLog, AuditLogFilter, CreateAuditLogParams};
use crate::domain::tenant::{PaginatedResult, Pagination};
use crate::error::Error;

#[derive(Clone)]
pub struct AuditRepository {
    pool: PgPool,
}

#[derive(Debug, sqlx::FromRow)]
struct AuditLogRow {
    id: Uuid,
    actor_type: String,
    actor_id: Option<Uuid>,
    action: String,
    resource_type: String,
    resource_id: Uuid,
    resource_name: String,
    detail: serde_json::Value,
    ip_address: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<AuditLogRow> for AuditLog {
    fn from(row: AuditLogRow) -> Self {
        AuditLog {
            id: row.id,
            actor_type: row.actor_type,
            actor_id: row.actor_id,
            action: row.action,
            resource_type: row.resource_type,
            resource_id: row.resource_id,
            resource_name: row.resource_name,
            detail: row.detail,
            ip_address: row.ip_address,
            created_at: row.created_at,
        }
    }
}

impl AuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, params: CreateAuditLogParams) -> Result<(), Error> {
        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (actor_type, actor_id, action, resource_type, resource_id, resource_name, detail, ip_address)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::inet)
            "#,
        )
        .bind(&params.actor_type)
        .bind(params.actor_id)
        .bind(&params.action)
        .bind(&params.resource_type)
        .bind(params.resource_id)
        .bind(&params.resource_name)
        .bind(&params.detail)
        .bind(&params.ip_address)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Failed to write audit log: {}", e)))?;

        Ok(())
    }

    pub async fn list(
        &self,
        filter: AuditLogFilter,
        pagination: Pagination,
    ) -> Result<PaginatedResult<AuditLog>, Error> {
        let pagination = pagination.capped();
        let page = pagination.page;
        let per_page = pagination.page_size;
        let offset = ((page - 1) * per_page) as i64;
        let limit = per_page as i64;

        let has_filters = filter.action.is_some()
            || filter.actor_type.is_some()
            || filter.actor_id.is_some()
            || filter.resource_type.is_some()
            || filter.resource_id.is_some()
            || filter.from.is_some()
            || filter.to.is_some();

        let (rows, total) = if !has_filters {
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_logs")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;
            let rows: Vec<AuditLogRow> = sqlx::query_as(
                "SELECT id, actor_type, actor_id, action, resource_type, resource_id, resource_name, detail, ip_address::text, created_at FROM audit_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;
            (rows, count.0)
        } else {
            self.list_filtered(filter, offset, limit).await?
        };

        let items = rows.into_iter().map(AuditLog::from).collect();
        Ok(PaginatedResult {
            items,
            total,
            page,
            page_size: per_page,
        })
    }

    async fn list_filtered(
        &self,
        filter: AuditLogFilter,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<AuditLogRow>, i64), Error> {
        let mut conditions = Vec::new();
        let mut bind_idx = 1u32;

        if filter.action.is_some() {
            conditions.push(format!("action = ${}", bind_idx));
            bind_idx += 1;
        }
        if filter.actor_type.is_some() {
            conditions.push(format!("actor_type = ${}", bind_idx));
            bind_idx += 1;
        }
        if filter.actor_id.is_some() {
            conditions.push(format!("actor_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if filter.resource_type.is_some() {
            conditions.push(format!("resource_type = ${}", bind_idx));
            bind_idx += 1;
        }
        if filter.resource_id.is_some() {
            conditions.push(format!("resource_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if filter.from.is_some() {
            conditions.push(format!("created_at >= ${}", bind_idx));
            bind_idx += 1;
        }
        if filter.to.is_some() {
            conditions.push(format!("created_at <= ${}", bind_idx));
            bind_idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };

        let count_sql = format!("SELECT COUNT(*) FROM audit_logs WHERE {}", where_clause);
        let data_sql = format!(
            "SELECT id, actor_type, actor_id, action, resource_type, resource_id, resource_name, detail, ip_address::text, created_at FROM audit_logs WHERE {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            where_clause, bind_idx, bind_idx + 1
        );

        // Build count query
        let mut count_query = sqlx::query_as::<_, (i64,)>(&count_sql);
        let mut data_query = sqlx::query_as::<_, AuditLogRow>(&data_sql);

        // Bind filter values in order
        if let Some(ref action) = filter.action {
            count_query = count_query.bind(action);
            data_query = data_query.bind(action);
        }
        if let Some(ref actor_type) = filter.actor_type {
            count_query = count_query.bind(actor_type);
            data_query = data_query.bind(actor_type);
        }
        if let Some(ref actor_id) = filter.actor_id {
            let uuid = Uuid::parse_str(actor_id)
                .map_err(|_| Error::InvalidParameter("Invalid actor_id".into()))?;
            count_query = count_query.bind(uuid);
            data_query = data_query.bind(uuid);
        }
        if let Some(ref resource_type) = filter.resource_type {
            count_query = count_query.bind(resource_type);
            data_query = data_query.bind(resource_type);
        }
        if let Some(resource_id) = filter.resource_id {
            count_query = count_query.bind(resource_id);
            data_query = data_query.bind(resource_id);
        }
        if let Some(ref from) = filter.from {
            count_query = count_query.bind(from);
            data_query = data_query.bind(from);
        }
        if let Some(ref to) = filter.to {
            count_query = count_query.bind(to);
            data_query = data_query.bind(to);
        }

        // Bind limit/offset for data query
        data_query = data_query.bind(limit).bind(offset);

        let count = count_query
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        let rows = data_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok((rows, count.0))
    }
}
