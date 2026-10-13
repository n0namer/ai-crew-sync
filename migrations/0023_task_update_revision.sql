-- Monotonic content revision and values-free mutation audit.
ALTER TABLE tasks ADD COLUMN revision BIGINT NOT NULL DEFAULT 1
    CONSTRAINT tasks_revision_positive CHECK (revision > 0);
CREATE TABLE task_update_audit (
    id BIGSERIAL PRIMARY KEY,
    task_id UUID NOT NULL,
    team_id UUID NOT NULL,
    actor_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    actor_session TEXT NOT NULL DEFAULT '',
    previous_revision BIGINT NOT NULL,
    new_revision BIGINT NOT NULL,
    changed_fields TEXT[] NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT task_update_audit_revisions CHECK (
        previous_revision > 0 AND new_revision = previous_revision + 1
    ),
    CONSTRAINT task_update_audit_same_team_task_fk
        FOREIGN KEY (team_id, task_id) REFERENCES tasks(team_id, id) ON DELETE CASCADE
);
CREATE INDEX task_update_audit_task_idx ON task_update_audit (task_id, id DESC);
