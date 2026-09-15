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

    if (this.authToken) {
      headers['Authorization'] = `Bearer ${this.authToken}`;
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
    return this.request<DashboardSummary>('/api/v1/dashboard/summary');
  }

  // Workflows
  async listWorkflows(status?: string): Promise<Workflow[]> {
    const query = status ? `?status=${encodeURIComponent(status)}` : '';
    return this.request<Workflow[]>(`/api/v1/workflows${query}`);
  }

  async createWorkflow(data: {
    name: string;
    description: string;
    auto_plan?: boolean;
    auto_start?: boolean;
  }): Promise<Workflow> {
    return this.request<Workflow>('/api/v1/workflows', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getWorkflow(id: string): Promise<Workflow> {
    return this.request<Workflow>(`/api/v1/workflows/${id}`);
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
  async getTaskDependencies(id: string): Promise<{ task_id: string; prerequisites: Task[]; dependents: Task[] }> {
    return this.request<{ task_id: string; prerequisites: Task[]; dependents: Task[] }>(
      `/api/v1/tasks/${id}/dependencies`
    );
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
}

export const api = new ApiClient();
