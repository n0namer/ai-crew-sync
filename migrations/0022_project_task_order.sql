-- Additive, team-scoped approved project orders for the global Mini Daemon.
-- Existing task rows are never implicitly enrolled.
CREATE TABLE project_task_orders (
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    project_key TEXT NOT NULL,
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, project_key)
);

-- PostgreSQL requires a matching UNIQUE key for the composite team FK.
CREATE UNIQUE INDEX tasks_team_id_id_for_project_order_idx ON tasks (team_id, id);

CREATE TABLE project_task_order_entries (
    team_id UUID NOT NULL,
    project_key TEXT NOT NULL,
    task_id UUID NOT NULL,
    position INT NOT NULL CHECK (position >= 1),
    PRIMARY KEY (team_id, project_key, task_id),
    UNIQUE (team_id, project_key, position),
    FOREIGN KEY (team_id, project_key)
        REFERENCES project_task_orders(team_id, project_key) ON DELETE CASCADE,
    FOREIGN KEY (team_id, task_id)
        REFERENCES tasks(team_id, id) ON DELETE CASCADE
);
CREATE INDEX project_task_order_next_idx
    ON project_task_order_entries (team_id, project_key, position);

-- Immutable approval history: replan must not rewrite a past revision.
CREATE TABLE project_task_order_revisions (
    team_id UUID NOT NULL,
    project_key TEXT NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    task_keys TEXT[] NOT NULL,
    approved_by UUID REFERENCES agents(id) ON DELETE SET NULL,
    approved_agent TEXT NOT NULL,
    approved_session TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id, project_key, revision),
    FOREIGN KEY (team_id, project_key)
        REFERENCES project_task_orders(team_id, project_key) ON DELETE CASCADE
);
