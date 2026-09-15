export interface SystemStatus {
  status: string;
  version: string;
  uptime_secs: number;
  agents_count: number;
  tools_count: number;
}

export interface AuthStatus {
  auth_required: boolean;
  authenticated: boolean;
}

export interface ProviderHealthInfo {
  provider_type: string;
  status: 'healthy' | 'degraded' | 'unavailable' | 'unknown';
  latency_ms?: number;
  error_count?: number;
  last_checked?: string;
}

export interface DashboardSummary {
  total_workflows: number;
  active_workflows: number;
  running_tasks: number;
  busy_agents: number;
  pending_approvals: number;
  failed_recoveries: number;
  latest_event_sequence: number;
  workflows: Workflow[];
  active_tasks: Task[];
  busy_agents_list: Agent[];
  pending_approvals_list: ApprovalRecord[];
  provider_health: Record<string, ProviderHealthInfo>;
}

export interface Workflow {
  id: string;
  name: string;
  description: string;
  state: 'Draft' | 'Planned' | 'Executing' | 'Paused' | 'Completed' | 'Failed' | 'Cancelled';
  created_at: string;
  updated_at: string;
  metadata?: Record<string, unknown>;
}

export interface Task {
  id: string;
  workflow_id: string;
  objective: string;
  description?: string | null;
  state:
    | 'Draft'
    | 'Pending'
    | 'Ready'
    | 'Assigned'
    | 'Running'
    | 'Verifying'
    | 'Verified'
    | 'Failed'
    | 'Recovering'
    | 'Blocked'
    | 'Cancelled'
    | 'Discarded';
  priority: number;
  assigned_agent_id?: string | null;
  lease_id?: string | null;
  required_capabilities: string[];
  created_at: string;
  updated_at: string;
}

export interface GraphNode {
  id: string;
  objective: string;
  description?: string | null;
  state: string;
  priority: number;
  assigned_agent?: string | null;
  assigned_agent_name?: string | null;
  required_capabilities: string[];
  has_verification: boolean;
  verification_passed?: boolean | null;
  created_at: string;
  updated_at: string;
}

export interface GraphEdge {
  from: string;
  to: string;
  kind: string;
}

export interface WorkflowGraph {
  workflow_id: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
  summary: {
    total: number;
    pending: number;
    running: number;
    verified: number;
    failed: number;
    blocked: number;
  };
}

export interface Agent {
  id: string;
  display_name: string;
  role: string;
  state: 'Idle' | 'Busy' | 'Paused' | 'Offline' | 'Terminated';
  capabilities: string[];
  current_task_id?: string | null;
  session_count: number;
  active_session_id?: string | null;
}

export interface ApprovalRecord {
  id: string;
  workflow_id: string;
  action_name: string;
  details: Record<string, unknown>;
  risk_level: 'Low' | 'Medium' | 'High' | 'Critical';
  status: 'Pending' | 'Approved' | 'Rejected';
  approver?: string | null;
  reason?: string | null;
  requested_at: string;
  decided_at?: string | null;
}

export interface Verification {
  id: string;
  task_id: string;
  workflow_id: string;
  passed: boolean;
  verdict: string;
  evidence: Record<string, unknown>;
  verified_at: string;
  verified_by?: string;
}

export interface ToolCallSummary {
  tool: string;
  input: unknown;
  output?: unknown;
  error?: string | null;
  duration_ms?: number;
}

export interface ExecutionRecord {
  id: string;
  task_id: string;
  agent_id: string;
  session_id?: string | null;
  attempt: number;
  status: string;
  tool_calls: ToolCallSummary[];
  output?: string | null;
  error?: string | null;
  started_at: string;
  finished_at?: string | null;
}

export interface AgentMessage {
  id: string;
  from_agent: string;
  to_agent: string;
  workflow_id: string;
  message_type: string;
  content: string;
  created_at: string;
}

export interface RecoveryRecord {
  id: string;
  task_id: string;
  workflow_id: string;
  attempt_number: number;
  strategy: string;
  diagnostics: string;
  status: string;
  created_at: string;
  completed_at?: string | null;
}

export interface EventRecord {
  sequence: number;
  event_id: string;
  event_type: string;
  workflow_id?: string | null;
  task_id?: string | null;
  agent_id?: string | null;
  timestamp: string;
  payload: Record<string, unknown>;
}

export interface ToolInfo {
  name: string;
  description: string;
  schema: unknown;
}

export interface MemoryRecord {
  id: string;
  scope: string;
  scope_id: string;
  key: string;
  content: string;
  metadata?: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}
