ALTER TABLE tasks
    ADD COLUMN completion_generation BIGINT NOT NULL DEFAULT 0
        CHECK (completion_generation >= 0);

CREATE FUNCTION crewsync_advance_completion_generation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.completion_generation := CASE WHEN NEW.status = 'claimed' THEN 1 ELSE 0 END;
    ELSIF ROW(NEW.status, NEW.claimed_by, NEW.claimed_session, NEW.claimed_at)
        IS DISTINCT FROM ROW(OLD.status, OLD.claimed_by, OLD.claimed_session, OLD.claimed_at) THEN
        NEW.completion_generation := OLD.completion_generation + 1;
    ELSE
        NEW.completion_generation := OLD.completion_generation;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER tasks_completion_generation
    BEFORE INSERT OR UPDATE ON tasks
    FOR EACH ROW EXECUTE FUNCTION crewsync_advance_completion_generation();

ALTER TABLE tasks ADD CONSTRAINT tasks_checked_original_binding_unique
    UNIQUE (team_id, id, key);

CREATE TABLE task_checked_completion_audit (
    id BIGSERIAL PRIMARY KEY,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    task_id UUID NOT NULL,
    task_key TEXT NOT NULL,
    attempt_id TEXT NOT NULL CHECK (length(attempt_id) BETWEEN 1 AND 256),
    crew_session_id TEXT NOT NULL CHECK (length(crew_session_id) BETWEEN 1 AND 256),
    owner_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    owner_session TEXT NOT NULL,
    owner_session_id UUID NOT NULL,
    owner_epoch BIGINT NOT NULL CHECK (owner_epoch > 0),
    owner_generation BIGINT NOT NULL CHECK (owner_generation > 0),
    source_sha256 TEXT NOT NULL CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
    result_sha256 TEXT NOT NULL CHECK (result_sha256 ~ '^[0-9a-f]{64}$'),
    reviewer_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    reviewer_session TEXT NOT NULL,
    reviewer_session_id UUID NOT NULL,
    reviewer_evidence_note_key TEXT NOT NULL,
    reviewer_evidence_sha256 TEXT NOT NULL CHECK (reviewer_evidence_sha256 ~ '^[0-9a-f]{64}$'),
    reducer_actor_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    reducer_session_id UUID NOT NULL,
    reducer_epoch BIGINT NOT NULL CHECK (reducer_epoch > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CONSTRAINT task_checked_audit_original_fk
        FOREIGN KEY (team_id, task_id, task_key)
        REFERENCES tasks (team_id, id, key) ON DELETE RESTRICT,
    CONSTRAINT task_checked_audit_independent_session
        CHECK (reviewer_session_id <> owner_session_id),
    CONSTRAINT task_checked_audit_independent_agent
        CHECK (reviewer_agent_id <> owner_agent_id),
    CONSTRAINT task_checked_audit_once UNIQUE (team_id, task_id)
);

CREATE FUNCTION crewsync_reject_checked_audit_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'checked completion audit is immutable' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER task_checked_completion_audit_immutable
    BEFORE UPDATE OR DELETE ON task_checked_completion_audit
    FOR EACH ROW EXECUTE FUNCTION crewsync_reject_checked_audit_mutation();

CREATE TRIGGER task_checked_completion_audit_no_truncate
    BEFORE TRUNCATE ON task_checked_completion_audit
    FOR EACH STATEMENT EXECUTE FUNCTION crewsync_reject_checked_audit_mutation();

REVOKE ALL ON task_checked_completion_audit FROM PUBLIC;
REVOKE ALL ON SEQUENCE task_checked_completion_audit_id_seq FROM PUBLIC;
