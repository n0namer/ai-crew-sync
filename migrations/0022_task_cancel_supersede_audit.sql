-- Durable cancellation and supersession state. A cancelled or superseded
-- prerequisite never counts as complete: only DONE resolves readiness.
-- Immutable audit rows and composite foreign keys enforce same-team identity.

ALTER TABLE tasks DROP CONSTRAINT tasks_status_check;
ALTER TABLE tasks ADD CONSTRAINT tasks_status_check
    CHECK (status IN ('open', 'claimed', 'done', 'cancelled', 'superseded'));

-- The task id is globally unique, but a composite key is needed so SQL
-- references cannot join two teams even if a future internal writer errs.
ALTER TABLE tasks ADD CONSTRAINT tasks_team_id_id_unique UNIQUE (team_id, id);

ALTER TABLE tasks
    ADD COLUMN cancellation_reason TEXT,
    ADD COLUMN superseded_by_task_id UUID,
    ADD COLUMN supersede_reason TEXT;

ALTER TABLE tasks ADD CONSTRAINT tasks_superseded_same_team_fk
    FOREIGN KEY (team_id, superseded_by_task_id)
    REFERENCES tasks (team_id, id) ON DELETE RESTRICT;

CREATE INDEX tasks_superseded_by_idx ON tasks (superseded_by_task_id);

CREATE TABLE task_transition_audit (
    id                  BIGSERIAL PRIMARY KEY,
    task_id             UUID NOT NULL,
    team_id             UUID NOT NULL REFERENCES teams(id) ON DELETE RESTRICT,
    actor_agent_id      UUID REFERENCES agents(id) ON DELETE SET NULL,
    actor_session       TEXT NOT NULL DEFAULT '',
    transition          TEXT NOT NULL CHECK (transition IN ('cancelled', 'superseded')),
    reason              TEXT NOT NULL,
    successor_task_id   UUID,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT task_transition_audit_same_team_task_fk
        FOREIGN KEY (team_id, task_id)
        REFERENCES tasks (team_id, id) ON DELETE RESTRICT,
    CONSTRAINT task_transition_audit_same_team_successor_fk
        FOREIGN KEY (team_id, successor_task_id)
        REFERENCES tasks (team_id, id) ON DELETE RESTRICT
);

CREATE INDEX task_transition_audit_task_idx
    ON task_transition_audit (task_id, id DESC);
