-- Daemon terminal authority is SEPARATE from legacy complete_task.
-- Only independently authenticated reviewer evidence may be inserted into
-- daemon_qa_acceptances by a future trusted service operation.
-- No trigger grants acceptance and no legacy task is migrated to this table.
ALTER TABLE agents ADD CONSTRAINT agents_team_id_id_unique UNIQUE (team_id, id);

CREATE TABLE daemon_execution_fences (
    team_id UUID NOT NULL,
    task_id UUID NOT NULL,
    source_revision BIGINT NOT NULL CHECK (source_revision > 0),
    source_sha TEXT NOT NULL CHECK (source_sha ~ '^[0-9a-f]{40}$'),
    attempt_id TEXT NOT NULL CHECK (length(attempt_id) BETWEEN 1 AND 256),
    owner_generation BIGINT NOT NULL CHECK (owner_generation > 0),
    fencing_token TEXT NOT NULL CHECK (length(fencing_token) BETWEEN 1 AND 256),
    owner_agent_id UUID NOT NULL,
    owner_session TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, task_id),
    CONSTRAINT daemon_execution_fences_task_fk FOREIGN KEY (team_id, task_id)
        REFERENCES tasks(team_id, id) ON DELETE CASCADE,
    CONSTRAINT daemon_execution_fences_agent_fk FOREIGN KEY (team_id, owner_agent_id)
        REFERENCES agents(team_id, id)
);

CREATE TABLE daemon_qa_acceptances (
    team_id UUID NOT NULL,
    task_id UUID NOT NULL,
    source_revision BIGINT NOT NULL CHECK (source_revision > 0),
    source_sha TEXT NOT NULL CHECK (source_sha ~ '^[0-9a-f]{40}$'),
    attempt_id TEXT NOT NULL CHECK (length(attempt_id) BETWEEN 1 AND 256),
    owner_generation BIGINT NOT NULL CHECK (owner_generation > 0),
    fencing_token TEXT NOT NULL CHECK (length(fencing_token) BETWEEN 1 AND 256),
    reviewer_agent_id UUID NOT NULL,
    reviewer_session TEXT NOT NULL,
    review_digest TEXT NOT NULL CHECK (review_digest ~ '^[0-9a-f]{64}$'),
    expires_at TIMESTAMPTZ NOT NULL,
    accepted BOOLEAN NOT NULL DEFAULT false,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, task_id, attempt_id),
    CONSTRAINT daemon_qa_acceptances_task_fk FOREIGN KEY (team_id, task_id)
        REFERENCES tasks(team_id, id) ON DELETE CASCADE,
    CONSTRAINT daemon_qa_acceptances_reviewer_fk FOREIGN KEY (team_id, reviewer_agent_id)
        REFERENCES agents(team_id, id),
    CONSTRAINT daemon_qa_acceptances_consume_accepted CHECK
        (consumed_at IS NULL OR accepted = true)
);

CREATE INDEX daemon_qa_acceptances_unconsumed_idx
    ON daemon_qa_acceptances (team_id, task_id)
    WHERE accepted = true AND consumed_at IS NULL;

-- Security gate: the application role must not gain arbitrary INSERT/UPDATE
-- on these two tables by deployment. Until a separately authenticated,
-- independent QA registration endpoint is provided, no live settlement
-- can succeed. Legacy completion remains unchanged.
