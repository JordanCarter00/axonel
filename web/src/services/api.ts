import {
  SystemStatus,
  AuthStatus,
  DashboardSummary,
  Workflow,
  Task,
  WorkflowGraph,
  Agent,
  ApprovalRecord,
  Verification,
  ExecutionRecord,
  AgentMessage,
  RecoveryRecord,
  EventRecord,
  ToolInfo,
  ProviderHealthInfo,
  MemoryRecord,
  Workspace,
  WorkspaceSecurityPolicy,
  GitStatusResponse,
  GitDiffResponse,
  GitCommitInfo,
  GitCommitResult,
  TaskTerminal,
  ProviderCapabilities,
  PruneRetentionResponse,
  GitHubRepoInfo,
  GitHubPullRequest,
  GitHubIssue,
  AgentBackendInfo,
} from '../types';

class ApiClient {
  private baseUrl: string = '';
  private authToken: string = '';

  constructor() {
    this.authToken = localStorage.getItem('plexis_auth_token') || '';
  }

  public setAuthToken(token: string) {
    this.authToken = token;
    localStorage.setItem('plexis_auth_token', token);
  }

  public getAuthToken(): string {
    return this.authToken;
  }

  private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...(options.headers as Record<string, string> || {}),
    };

    const token = this.authToken || (typeof localStorage !== 'undefined' ? localStorage.getItem('plexis_auth_token') : null);
    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
    }

    const url = `${this.baseUrl}${path}`;
    const response = await fetch(url, {
      ...options,
      headers,
    });

    if (!response.ok) {
      let errorMessage = `HTTP ${response.status}: ${response.statusText}`;
      try {
        const errorJson = await response.json();
        if (errorJson.error) {
          errorMessage = errorJson.error;
        }
      } catch {
        // Fallback to text or status text
      }
      throw new Error(errorMessage);
    }

    return response.json();
  }

  // System & Auth
  async getSystemStatus(): Promise<SystemStatus> {
    return this.request<SystemStatus>('/api/v1/system/status');
  }

  async getAuthStatus(): Promise<AuthStatus> {
    return this.request<AuthStatus>('/api/v1/auth/status');
  }

  // Dashboard
  async getDashboardSummary(): Promise<DashboardSummary> {
    const summary = await this.request<DashboardSummary>('/api/v1/dashboard/summary');
    if (summary.workflows) {
      summary.workflows = summary.workflows.map((w) => this.normalizeWorkflow(w));
    }
    return summary;
  }

  // Workflows
  private normalizeWorkflow(w: Workflow): Workflow {
    return {
      ...w,
      name: w.name || w.title || 'Untitled Workflow',
      title: w.title || w.name || 'Untitled Workflow',
      description: w.description || w.objective || '',
      objective: w.objective || w.description || '',
    };
  }

  async listWorkflows(status?: string, workspaceId?: string): Promise<Workflow[]> {
    const searchParams = new URLSearchParams();
    if (status) searchParams.set('status', status);
    if (workspaceId) searchParams.set('workspace_id', workspaceId);
    const query = searchParams.toString() ? `?${searchParams.toString()}` : '';
    const list = await this.request<Workflow[]>(`/api/v1/workflows${query}`);
    return list.map((w) => this.normalizeWorkflow(w));
  }

  async createWorkflow(data: {
    name?: string;
    title?: string;
    description?: string;
    objective?: string;
    workspace_id?: string | null;
    auto_plan?: boolean;
    auto_start?: boolean;
    backend?: string;
  }): Promise<Workflow> {
    const payload = {
      title: data.title || data.name || '',
      objective: data.objective || data.description || '',
      workspace_id: data.workspace_id,
      auto_plan: data.auto_plan ?? true,
      auto_start: data.auto_start ?? true,
      backend: data.backend,
    };
    const created = await this.request<Workflow>('/api/v1/workflows', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
    return this.normalizeWorkflow(created);
  }

  async getWorkflow(id: string): Promise<Workflow> {
    const wf = await this.request<Workflow>(`/api/v1/workflows/${id}`);
    return this.normalizeWorkflow(wf);
  }

  async getWorkflowTasks(id: string): Promise<Task[]> {
    return this.request<Task[]>(`/api/v1/workflows/${id}/tasks`);
  }

  async getWorkflowGraph(id: string): Promise<WorkflowGraph> {
    return this.request<WorkflowGraph>(`/api/v1/workflows/${id}/graph`);
  }

  async planWorkflow(id: string): Promise<{ success: boolean; tasks_count: number }> {
    return this.request<{ success: boolean; tasks_count: number }>(`/api/v1/workflows/${id}/plan`, {
      method: 'POST',
    });
  }

  async startWorkflow(id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/workflows/${id}/start`, {
      method: 'POST',
    });
  }

  async pauseWorkflow(id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/workflows/${id}/pause`, {
      method: 'POST',
    });
  }

  async resumeWorkflow(id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/workflows/${id}/resume`, {
      method: 'POST',
    });
  }

  async cancelWorkflow(id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/workflows/${id}/cancel`, {
      method: 'POST',
    });
  }

  async getWorkflowMessages(id: string): Promise<AgentMessage[]> {
    return this.request<AgentMessage[]>(`/api/v1/workflows/${id}/messages`);
  }

  async getWorkflowRecoveries(id: string): Promise<RecoveryRecord[]> {
    return this.request<RecoveryRecord[]>(`/api/v1/workflows/${id}/recoveries`);
  }

  async getWorkflowVerifications(id: string): Promise<Verification[]> {
    return this.request<Verification[]>(`/api/v1/workflows/${id}/verifications`);
  }

  // Tasks
  async getTask(id: string): Promise<Task> {
    return this.request<Task>(`/api/v1/tasks/${id}`);
  }

  async getTaskDependencies(id: string): Promise<{ task_id: string; prerequisites: Task[]; dependents: Task[] }> {
    const res = await this.request<{
      task_id?: string;
      dependencies?: Task[];
      prerequisites?: Task[];
      dependents?: Task[];
    }>(`/api/v1/tasks/${id}/dependencies`);
    return {
      task_id: res.task_id || id,
      prerequisites: res.prerequisites || res.dependencies || [],
      dependents: res.dependents || [],
    };
  }

  async getTaskExecutions(id: string): Promise<ExecutionRecord[]> {
    return this.request<ExecutionRecord[]>(`/api/v1/tasks/${id}/executions`);
  }

  async getTaskVerifications(id: string): Promise<Verification[]> {
    return this.request<Verification[]>(`/api/v1/tasks/${id}/verifications`);
  }

  async getTaskMessages(id: string): Promise<AgentMessage[]> {
    return this.request<AgentMessage[]>(`/api/v1/tasks/${id}/messages`);
  }

  async getTaskRecoveries(id: string): Promise<RecoveryRecord[]> {
    return this.request<RecoveryRecord[]>(`/api/v1/tasks/${id}/recoveries`);
  }

  async reassignTask(id: string, agent_id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/tasks/${id}/reassign`, {
      method: 'POST',
      body: JSON.stringify({ agent_id }),
    });
  }

  // Agents
  async listAgents(): Promise<Agent[]> {
    return this.request<Agent[]>('/api/v1/agents');
  }

  async getAgent(id: string): Promise<Agent> {
    return this.request<Agent>(`/api/v1/agents/${id}`);
  }

  async getAgentSessions(id: string): Promise<unknown[]> {
    return this.request<unknown[]>(`/api/v1/agents/${id}/sessions`);
  }

  async getAgentExecutions(id: string): Promise<ExecutionRecord[]> {
    return this.request<ExecutionRecord[]>(`/api/v1/agents/${id}/executions`);
  }

  async getAgentMessages(id: string): Promise<AgentMessage[]> {
    return this.request<AgentMessage[]>(`/api/v1/agents/${id}/messages`);
  }

  async pauseAgent(id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/agents/${id}/pause`, {
      method: 'POST',
    });
  }

  async resumeAgent(id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/agents/${id}/resume`, {
      method: 'POST',
    });
  }

  async cancelAgent(id: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/agents/${id}/cancel`, {
      method: 'POST',
    });
  }

  async sendAgentMessage(
    id: string,
    message: { to_agent: string; workflow_id: string; message_type: string; content: string }
  ): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/agents/${id}/message`, {
      method: 'POST',
      body: JSON.stringify(message),
    });
  }

  // Approvals
  async listApprovals(status?: 'all' | 'pending' | 'approved' | 'rejected'): Promise<ApprovalRecord[]> {
    const query = status ? `?status=${encodeURIComponent(status)}` : '';
    return this.request<ApprovalRecord[]>(`/api/v1/approvals${query}`);
  }

  async approve(id: string, notes?: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/approvals/${id}/approve`, {
      method: 'POST',
      body: JSON.stringify({ notes }),
    });
  }

  async reject(id: string, reason?: string): Promise<{ success: boolean }> {
    return this.request<{ success: boolean }>(`/api/v1/approvals/${id}/reject`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    });
  }

  // Events
  async listEvents(params?: {
    after?: number;
    limit?: number;
    workflow_id?: string;
    agent_id?: string;
    event_type?: string;
  }): Promise<EventRecord[]> {
    const searchParams = new URLSearchParams();
    if (params?.after !== undefined) searchParams.set('after', params.after.toString());
    if (params?.limit !== undefined) searchParams.set('limit', params.limit.toString());
    if (params?.workflow_id) searchParams.set('workflow_id', params.workflow_id);
    if (params?.agent_id) searchParams.set('agent_id', params.agent_id);
    if (params?.event_type) searchParams.set('event_type', params.event_type);

    const query = searchParams.toString() ? `?${searchParams.toString()}` : '';
    return this.request<EventRecord[]>(`/api/v1/events${query}`);
  }

  async getMissedEvents(after: number, limit = 1000): Promise<{ events: EventRecord[]; count: number; cursor: number }> {
    return this.request<{ events: EventRecord[]; count: number; cursor: number }>(
      `/api/v1/events/cursor?after=${after}&limit=${limit}`
    );
  }

  // Tools, Memory, Providers
  async listTools(): Promise<ToolInfo[]> {
    return this.request<ToolInfo[]>('/api/v1/tools');
  }

  async listProviders(): Promise<Record<string, ProviderHealthInfo>> {
    return this.request<Record<string, ProviderHealthInfo>>('/api/v1/providers');
  }

  async listMemories(scope?: string, scope_id?: string): Promise<MemoryRecord[]> {
    const searchParams = new URLSearchParams();
    if (scope) searchParams.set('scope', scope);
    if (scope_id) searchParams.set('scope_id', scope_id);
    const query = searchParams.toString() ? `?${searchParams.toString()}` : '';
    return this.request<MemoryRecord[]>(`/api/v1/memories${query}`);
  }

  // Workspaces
  async listWorkspaces(): Promise<Workspace[]> {
    return this.request<Workspace[]>('/api/v1/workspaces');
  }

  async createWorkspace(payload: {
    name: string;
    canonical_path: string;
    description?: string;
    is_default?: boolean;
    security_policy?: WorkspaceSecurityPolicy;
  }): Promise<Workspace> {
    return this.request<Workspace>('/api/v1/workspaces', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  async getWorkspace(id: string): Promise<Workspace> {
    return this.request<Workspace>(`/api/v1/workspaces/${id}`);
  }

  async updateWorkspace(
    id: string,
    payload: {
      name?: string;
      description?: string;
      security_policy?: WorkspaceSecurityPolicy;
    }
  ): Promise<Workspace> {
    return this.request<Workspace>(`/api/v1/workspaces/${id}`, {
      method: 'PUT',
      body: JSON.stringify(payload),
    });
  }

  async deleteWorkspace(id: string): Promise<void> {
    await this.request<void>(`/api/v1/workspaces/${id}`, {
      method: 'DELETE',
    });
  }

  // Workspace Git Operations
  async getWorkspaceGitStatus(workspaceId: string): Promise<GitStatusResponse> {
    return this.request<GitStatusResponse>(`/api/v1/workspaces/${workspaceId}/git/status`);
  }

  async getWorkspaceGitDiff(workspaceId: string, staged?: boolean): Promise<GitDiffResponse> {
    const query = staged !== undefined ? `?staged=${staged}` : '';
    return this.request<GitDiffResponse>(`/api/v1/workspaces/${workspaceId}/git/diff${query}`);
  }

  async getWorkspaceGitLog(workspaceId: string, limit = 20): Promise<GitCommitInfo[]> {
    return this.request<GitCommitInfo[]>(`/api/v1/workspaces/${workspaceId}/git/log?limit=${limit}`);
  }

  async commitWorkspaceGit(workspaceId: string, message: string): Promise<GitCommitResult> {
    return this.request<GitCommitResult>(`/api/v1/workspaces/${workspaceId}/git/commit`, {
      method: 'POST',
      body: JSON.stringify({ message }),
    });
  }

  async getGitDiff(workspaceId: string, staged?: boolean): Promise<GitDiffResponse> {
    return this.getWorkspaceGitDiff(workspaceId, staged);
  }

  async commitGit(workspaceId: string, message: string): Promise<GitCommitResult> {
    return this.commitWorkspaceGit(workspaceId, message);
  }

  // Task Terminal Streaming
  async getTaskTerminal(taskId: string): Promise<TaskTerminal> {
    return this.request<TaskTerminal>(`/api/v1/tasks/${taskId}/terminal`);
  }

  // Provider Capabilities
  async getProviderCapabilities(): Promise<ProviderCapabilities[]> {
    return this.request<ProviderCapabilities[]>('/api/v1/providers/capabilities');
  }

  // Retention
  async pruneRetentionRecords(maxAgeDays?: number): Promise<PruneRetentionResponse> {
    return this.request<PruneRetentionResponse>('/api/v1/retention/prune', {
      method: 'POST',
      body: JSON.stringify({ max_age_days: maxAgeDays }),
    });
  }

  // GitHub Integration
  async listGitHubRepos(): Promise<GitHubRepoInfo[]> {
    return this.request<GitHubRepoInfo[]>('/api/v1/github/repos');
  }

  async listGitHubPulls(repo: string): Promise<GitHubPullRequest[]> {
    return this.request<GitHubPullRequest[]>(`/api/v1/github/pulls?repo=${encodeURIComponent(repo)}`);
  }

  async createGitHubPull(payload: {
    title: string;
    body: string;
    head: string;
    base: string;
  }): Promise<GitHubPullRequest> {
    return this.request<GitHubPullRequest>('/api/v1/github/pulls', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  async listGitHubIssues(repo: string): Promise<GitHubIssue[]> {
    return this.request<GitHubIssue[]>(`/api/v1/github/issues?repo=${encodeURIComponent(repo)}`);
  }

  // Milestone 12: Local Agent Host & External Process Control
  async listAgentBackends(): Promise<AgentBackendInfo[]> {
    const res = await this.request<{ backends?: AgentBackendInfo[] } | AgentBackendInfo[]>(
      '/api/v1/agent-host/backends'
    );
    return Array.isArray(res) ? res : (res.backends || []);
  }

  async listAgentExecutions(): Promise<ExecutionRecord[]> {
    return this.request<ExecutionRecord[]>('/api/v1/agent-host/executions');
  }

  async cancelAgentExecution(id: string): Promise<{ success: boolean; message: string }> {
    return this.request<{ success: boolean; message: string }>(`/api/v1/agent-host/executions/${id}/cancel`, {
      method: 'POST',
    });
  }
}

export const api = new ApiClient();
