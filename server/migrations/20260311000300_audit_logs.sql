-- Audit logs table
CREATE TABLE audit_logs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_type    VARCHAR(16) NOT NULL CHECK(actor_type IN ('admin', 'tenant')),
    actor_id      UUID,
    action        VARCHAR(64) NOT NULL,
    resource_type VARCHAR(32) NOT NULL,
    resource_id   UUID NOT NULL,
    resource_name VARCHAR(255) NOT NULL DEFAULT '',
    detail        JSONB NOT NULL DEFAULT '{}',
    ip_address    INET,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at);
CREATE INDEX idx_audit_logs_actor ON audit_logs(actor_type, actor_id);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX idx_audit_logs_query ON audit_logs(created_at, action, actor_type);
