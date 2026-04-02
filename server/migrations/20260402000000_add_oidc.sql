-- OIDC SSO integration schema

-- OIDC configuration (singleton, id=1)
CREATE TABLE oidc_config (
    id                    INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    enabled               BOOLEAN NOT NULL DEFAULT false,
    issuer_url            TEXT NOT NULL DEFAULT '',
    client_id             TEXT NOT NULL DEFAULT '',
    client_secret_encrypted TEXT NOT NULL DEFAULT '',  -- AES-256-GCM encrypted
    redirect_uri          TEXT NOT NULL DEFAULT '',
    jwks_refresh_interval_secs INTEGER NOT NULL DEFAULT 3600,
    disable_password_login BOOLEAN NOT NULL DEFAULT false,
    auto_create_tenant    BOOLEAN NOT NULL DEFAULT false,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OIDC authorization sessions (PKCE + state/nonce, 10min expiry)
CREATE TABLE oidc_auth_sessions (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    state         VARCHAR(64) NOT NULL,
    nonce         VARCHAR(64) NOT NULL,
    code_verifier TEXT NOT NULL,
    consumed      BOOLEAN NOT NULL DEFAULT false,
    ip_address    INET,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '10 minutes')
);
CREATE UNIQUE INDEX idx_oidc_auth_sessions_state ON oidc_auth_sessions(state);
CREATE INDEX idx_oidc_auth_sessions_expires ON oidc_auth_sessions(expires_at);

-- OIDC token store (ElevoOne tokens + session_code, 7-day expiry)
CREATE TABLE oidc_token_store (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    local_session_id UUID NOT NULL UNIQUE,       -- links to our JWT session_id
    user_id       BIGINT NOT NULL,               -- ElevoOne user_id (from claims.sub)
    org_id        BIGINT,                        -- ElevoOne org_id
    org_role      VARCHAR(50),                   -- ElevoOne org_role
    email         VARCHAR(255),
    name          VARCHAR(255),
    picture       VARCHAR(1024),
    local_jwt     TEXT NOT NULL,                 -- our admin JWT
    access_token  TEXT,                          -- ElevoOne access_token
    refresh_token TEXT,
    id_token      TEXT,
    session_code  VARCHAR(64),                   -- one-time code for callback exchange (NULL after consumed)
    session_code_consumed BOOLEAN NOT NULL DEFAULT false,
    session_code_expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '30 seconds'),
    ip_address    INET,
    last_refreshed_at TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL DEFAULT (now() + INTERVAL '7 days')
);
CREATE INDEX idx_oidc_token_store_session ON oidc_token_store(local_session_id);
CREATE UNIQUE INDEX idx_oidc_token_store_code ON oidc_token_store(session_code) WHERE session_code IS NOT NULL;
CREATE INDEX idx_oidc_token_store_expires ON oidc_token_store(expires_at);
CREATE INDEX idx_oidc_token_store_user_org ON oidc_token_store(user_id, org_id);

-- Add ElevoOne org_id mapping to tenants
ALTER TABLE tenants ADD COLUMN elevoone_org_id BIGINT;
CREATE UNIQUE INDEX idx_tenants_elevoone_org_id ON tenants(elevoone_org_id) WHERE elevoone_org_id IS NOT NULL;

-- Extend audit_logs actor_type to include 'anonymous'
ALTER TABLE audit_logs DROP CONSTRAINT audit_logs_actor_type_check;
ALTER TABLE audit_logs ADD CONSTRAINT audit_logs_actor_type_check
    CHECK(actor_type IN ('admin', 'tenant', 'anonymous'));
