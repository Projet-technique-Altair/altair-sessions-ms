CREATE TABLE IF NOT EXISTS lab_terminal_events (
  event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  session_id UUID NOT NULL,
  runtime_id UUID,
  user_id UUID NOT NULL,
  lab_id UUID NOT NULL,
  occurred_at TIMESTAMP DEFAULT timezone('UTC'::text, now()) NOT NULL,
  command_redacted TEXT NOT NULL,
  exit_status INT,

  CONSTRAINT fk_lab_terminal_events_session
    FOREIGN KEY (session_id)
    REFERENCES lab_sessions(session_id)
    ON DELETE CASCADE,

  CONSTRAINT fk_lab_terminal_events_runtime
    FOREIGN KEY (runtime_id)
    REFERENCES lab_session_runtimes(runtime_id)
    ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_lab_terminal_events_session
  ON lab_terminal_events(session_id);
CREATE INDEX IF NOT EXISTS idx_lab_terminal_events_runtime
  ON lab_terminal_events(runtime_id);
CREATE INDEX IF NOT EXISTS idx_lab_terminal_events_user
  ON lab_terminal_events(user_id);
CREATE INDEX IF NOT EXISTS idx_lab_terminal_events_lab
  ON lab_terminal_events(lab_id);
CREATE INDEX IF NOT EXISTS idx_lab_terminal_events_occurred_at
  ON lab_terminal_events(occurred_at);
CREATE INDEX IF NOT EXISTS idx_lab_terminal_events_session_occurred_at
  ON lab_terminal_events(session_id, occurred_at);

CREATE TABLE IF NOT EXISTS lab_validation_events (
  event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  session_id UUID NOT NULL,
  user_id UUID NOT NULL,
  lab_id UUID NOT NULL,
  step_number INT NOT NULL,
  attempt_index INT NOT NULL,
  submitted_at TIMESTAMP DEFAULT timezone('UTC'::text, now()) NOT NULL,
  answer_redacted TEXT,
  answer_hash TEXT,
  is_correct BOOLEAN NOT NULL,
  validation_type VARCHAR(100),

  CONSTRAINT fk_lab_validation_events_session
    FOREIGN KEY (session_id)
    REFERENCES lab_sessions(session_id)
    ON DELETE CASCADE,

  CONSTRAINT chk_lab_validation_events_attempt_index
    CHECK (attempt_index >= 1),

  CONSTRAINT chk_lab_validation_events_step_number
    CHECK (step_number >= 1)
);

CREATE INDEX IF NOT EXISTS idx_lab_validation_events_session
  ON lab_validation_events(session_id);
CREATE INDEX IF NOT EXISTS idx_lab_validation_events_user
  ON lab_validation_events(user_id);
CREATE INDEX IF NOT EXISTS idx_lab_validation_events_lab
  ON lab_validation_events(lab_id);
CREATE INDEX IF NOT EXISTS idx_lab_validation_events_step
  ON lab_validation_events(session_id, step_number);
CREATE INDEX IF NOT EXISTS idx_lab_validation_events_submitted_at
  ON lab_validation_events(submitted_at);

CREATE TABLE IF NOT EXISTS lab_hint_events (
  event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  session_id UUID NOT NULL,
  user_id UUID NOT NULL,
  lab_id UUID NOT NULL,
  step_number INT NOT NULL,
  hint_id UUID,
  hint_number INT,
  requested_at TIMESTAMP DEFAULT timezone('UTC'::text, now()) NOT NULL,

  CONSTRAINT fk_lab_hint_events_session
    FOREIGN KEY (session_id)
    REFERENCES lab_sessions(session_id)
    ON DELETE CASCADE,

  CONSTRAINT chk_lab_hint_events_step_number
    CHECK (step_number >= 1),

  CONSTRAINT chk_lab_hint_events_hint_number
    CHECK (hint_number IS NULL OR hint_number >= 1)
);

CREATE INDEX IF NOT EXISTS idx_lab_hint_events_session
  ON lab_hint_events(session_id);
CREATE INDEX IF NOT EXISTS idx_lab_hint_events_user
  ON lab_hint_events(user_id);
CREATE INDEX IF NOT EXISTS idx_lab_hint_events_lab
  ON lab_hint_events(lab_id);
CREATE INDEX IF NOT EXISTS idx_lab_hint_events_step
  ON lab_hint_events(session_id, step_number);
CREATE INDEX IF NOT EXISTS idx_lab_hint_events_requested_at
  ON lab_hint_events(requested_at);

CREATE TABLE IF NOT EXISTS ai_analysis_requests (
  analysis_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  report_type VARCHAR(100) NOT NULL,
  requested_by_user_id UUID NOT NULL,
  lab_id UUID NOT NULL,
  student_user_id UUID,
  group_id UUID,
  session_id UUID,
  status VARCHAR(50) NOT NULL DEFAULT 'pending',
  model_provider VARCHAR(50) NOT NULL,
  model_name VARCHAR(100) NOT NULL,
  created_at TIMESTAMP DEFAULT timezone('UTC'::text, now()) NOT NULL,
  finished_at TIMESTAMP,

  CONSTRAINT fk_ai_analysis_requests_session
    FOREIGN KEY (session_id)
    REFERENCES lab_sessions(session_id)
    ON DELETE SET NULL,

  CONSTRAINT chk_ai_analysis_requests_status
    CHECK (status IN ('pending', 'running', 'completed', 'failed')),

  CONSTRAINT chk_ai_analysis_requests_report_type
    CHECK (report_type IN ('individual_student_activity_report', 'group_activity_report'))
);

CREATE INDEX IF NOT EXISTS idx_ai_analysis_requests_lab
  ON ai_analysis_requests(lab_id);
CREATE INDEX IF NOT EXISTS idx_ai_analysis_requests_requested_by
  ON ai_analysis_requests(requested_by_user_id);
CREATE INDEX IF NOT EXISTS idx_ai_analysis_requests_student
  ON ai_analysis_requests(student_user_id);
CREATE INDEX IF NOT EXISTS idx_ai_analysis_requests_group
  ON ai_analysis_requests(group_id);
CREATE INDEX IF NOT EXISTS idx_ai_analysis_requests_status
  ON ai_analysis_requests(status);
CREATE INDEX IF NOT EXISTS idx_ai_analysis_requests_created_at
  ON ai_analysis_requests(created_at DESC);
