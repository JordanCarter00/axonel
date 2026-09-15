import React from 'react';
import {
  Layers,
  CheckCircle2,
  AlertTriangle,
  Play,
  Clock,
  ShieldAlert,
  ArrowRight,
  RefreshCw,
  Cpu,
} from 'lucide-react';
import { DashboardSummary, Workflow } from '../types';

interface DashboardViewProps {
  summary: DashboardSummary | null;
  loading: boolean;
  onSelectWorkflow: (id: string) => void;
  onOpenApprovals: () => void;
  onOpenNewWorkflow: () => void;
  onRefresh: () => void;
}

export const DashboardView: React.FC<DashboardViewProps> = ({
  summary,
  loading,
  onSelectWorkflow,
  onOpenApprovals,
  onOpenNewWorkflow,
  onRefresh,
}) => {
  if (loading && !summary) {
    return (
      <div className="flex items-center justify-center min-h-[60vh]">
        <div className="flex flex-col items-center space-y-3">
          <RefreshCw className="w-6 h-6 animate-spin text-primary-400" />
          <span className="text-sm font-mono text-slate-400">Loading system state...</span>
        </div>
      </div>
    );
  }

  const s = summary || {
    total_workflows: 0,
    active_workflows: 0,
    running_tasks: 0,
    busy_agents: 0,
    pending_approvals: 0,
    failed_recoveries: 0,
    latest_event_sequence: 0,
    workflows: [],
    active_tasks: [],
    busy_agents_list: [],
    pending_approvals_list: [],
    provider_health: {},
  };

  const getStatusColor = (state: string) => {
    switch (state.toLowerCase()) {
      case 'executing':
      case 'running':
        return 'bg-sky-500/10 text-sky-400 border-sky-500/30';
      case 'completed':
      case 'verified':
        return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
      case 'planned':
        return 'bg-indigo-500/10 text-indigo-400 border-indigo-500/30';
      case 'failed':
        return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      case 'recovering':
        return 'bg-amber-500/10 text-amber-400 border-amber-500/30';
      case 'paused':
        return 'bg-yellow-500/10 text-yellow-400 border-yellow-500/30';
      default:
        return 'bg-slate-800 text-slate-400 border-slate-700';
    }
  };

  return (
    <div className="space-y-6 pb-12">
      {/* Top Banner Alert if Pending Approvals */}
      {s.pending_approvals > 0 && (
        <div className="bg-amber-950/40 border border-amber-500/40 rounded-lg p-4 flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <div className="p-2 bg-amber-500/20 rounded-md text-amber-400">
              <ShieldAlert className="w-5 h-5 animate-pulse" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-amber-200">
                {s.pending_approvals} Action{s.pending_approvals > 1 ? 's' : ''} Awaiting Human Approval
              </h3>
              <p className="text-xs text-amber-300/80">
                Autonomous execution is safely paused on risky operations until an operator reviews them.
              </p>
            </div>
          </div>
          <button
            onClick={onOpenApprovals}
            className="px-3.5 py-1.5 bg-amber-500 hover:bg-amber-400 text-slate-950 rounded-md text-xs font-semibold flex items-center space-x-1.5 transition-colors"
          >
            <span>Review Approvals</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* KPI Metric Cards */}
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3">
        <div className="bg-surface border border-surface-border rounded-lg p-4">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs font-medium uppercase tracking-wider">Workflows</span>
            <Layers className="w-4 h-4 text-indigo-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100">{s.total_workflows}</div>
          <div className="text-[11px] text-slate-400 mt-1 font-mono">
            <span className="text-sky-400 font-semibold">{s.active_workflows}</span> active
          </div>
        </div>

        <div className="bg-surface border border-surface-border rounded-lg p-4">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs font-medium uppercase tracking-wider">Running Tasks</span>
            <Play className="w-4 h-4 text-sky-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100">{s.running_tasks}</div>
          <div className="text-[11px] text-slate-400 mt-1 font-mono">parallel leases active</div>
        </div>

        <div className="bg-surface border border-surface-border rounded-lg p-4">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs font-medium uppercase tracking-wider">Busy Agents</span>
            <Cpu className="w-4 h-4 text-purple-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100">{s.busy_agents}</div>
          <div className="text-[11px] text-slate-400 mt-1 font-mono">
            {s.busy_agents_list.length} registered
          </div>
        </div>

        <div className="bg-surface border border-surface-border rounded-lg p-4">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs font-medium uppercase tracking-wider">Approvals</span>
            <ShieldAlert className="w-4 h-4 text-amber-400" />
          </div>
          <div
            className={`text-2xl font-bold font-mono ${
              s.pending_approvals > 0 ? 'text-amber-400' : 'text-slate-100'
            }`}
          >
            {s.pending_approvals}
          </div>
          <div className="text-[11px] text-slate-400 mt-1 font-mono">governance gates</div>
        </div>

        <div className="bg-surface border border-surface-border rounded-lg p-4">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs font-medium uppercase tracking-wider">Recoveries</span>
            <AlertTriangle className="w-4 h-4 text-rose-400" />
          </div>
          <div
            className={`text-2xl font-bold font-mono ${
              s.failed_recoveries > 0 ? 'text-rose-400' : 'text-slate-100'
            }`}
          >
            {s.failed_recoveries}
          </div>
          <div className="text-[11px] text-slate-400 mt-1 font-mono">active / escalated</div>
        </div>

        <div className="bg-surface border border-surface-border rounded-lg p-4">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs font-medium uppercase tracking-wider">Audit Log</span>
            <Clock className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-2xl font-bold font-mono text-slate-100">{s.latest_event_sequence}</div>
          <div className="text-[11px] text-slate-400 mt-1 font-mono">immutable events</div>
        </div>
      </div>

      {/* Main Section: Recent Workflows & Active Tasks */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left 2 Cols: Workflows */}
        <div className="lg:col-span-2 space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-300 flex items-center space-x-2">
              <Layers className="w-4 h-4 text-indigo-400" />
              <span>Recent Workflows</span>
            </h2>
            <div className="flex items-center space-x-3">
              <button
                onClick={onRefresh}
                className="text-xs text-slate-400 hover:text-slate-200 flex items-center space-x-1"
                title="Refresh dashboard state"
              >
                <RefreshCw className="w-3.5 h-3.5" />
                <span>Refresh</span>
              </button>
              <button
                onClick={onOpenNewWorkflow}
                className="text-xs text-primary-400 hover:text-primary-300 font-medium flex items-center space-x-1"
              >
                <span>+ Plan New</span>
              </button>
            </div>
          </div>

          <div className="bg-surface border border-surface-border rounded-lg overflow-hidden divide-y divide-surface-border">
            {s.workflows.length === 0 ? (
              <div className="p-8 text-center">
                <p className="text-sm text-slate-400 mb-3">No workflows registered yet.</p>
                <button
                  onClick={onOpenNewWorkflow}
                  className="px-3 py-1.5 bg-primary-600 hover:bg-primary-500 text-white rounded text-xs font-medium"
                >
                  Create First Workflow
                </button>
              </div>
            ) : (
              s.workflows.slice(0, 5).map((w: Workflow) => (
                <div
                  key={w.id}
                  onClick={() => onSelectWorkflow(w.id)}
                  className="p-4 hover:bg-surface-hover cursor-pointer transition-colors flex items-center justify-between"
                >
                  <div className="space-y-1">
                    <div className="flex items-center space-x-2">
                      <span className="font-semibold text-slate-200 text-sm">{w.name}</span>
                      <span
                        className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${getStatusColor(
                          w.state
                        )}`}
                      >
                        {w.state}
                      </span>
                    </div>
                    <p className="text-xs text-slate-400 line-clamp-1">{w.description}</p>
                    <div className="text-[10px] font-mono text-slate-500">ID: {w.id.slice(0, 8)}...</div>
                  </div>
                  <div className="flex items-center space-x-3 text-slate-400">
                    <span className="text-xs font-mono">
                      {new Date(w.created_at).toLocaleTimeString()}
                    </span>
                    <ArrowRight className="w-4 h-4 text-slate-500 hover:text-slate-200" />
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        {/* Right 1 Col: Provider Health Matrix & Active Agents */}
        <div className="space-y-6">
          {/* Provider Health */}
          <div>
            <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-300 mb-3 flex items-center space-x-2">
              <CheckCircle2 className="w-4 h-4 text-emerald-400" />
              <span>Provider Health Matrix</span>
            </h2>
            <div className="bg-surface border border-surface-border rounded-lg p-4 space-y-3">
              {Object.keys(s.provider_health).length === 0 ? (
                <div className="text-xs text-slate-400 font-mono">No providers registered yet</div>
              ) : (
                Object.entries(s.provider_health).map(([name, info]) => (
                  <div key={name} className="flex items-center justify-between border-b border-surface-border pb-2 last:border-0 last:pb-0">
                    <div>
                      <div className="text-xs font-semibold text-slate-200">{name}</div>
                      <div className="text-[10px] font-mono text-slate-400">{info.provider_type}</div>
                    </div>
                    <div className="text-right">
                      <span
                        className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${
                          info.status === 'healthy'
                            ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                            : 'bg-rose-500/10 text-rose-400 border-rose-500/30'
                        }`}
                      >
                        {info.status}
                      </span>
                      {info.latency_ms !== undefined && (
                        <div className="text-[10px] font-mono text-slate-500 mt-0.5">
                          {info.latency_ms}ms
                        </div>
                      )}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>

          {/* Active Tasks & Parallel Leases */}
          <div>
            <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-300 mb-3 flex items-center space-x-2">
              <Play className="w-4 h-4 text-sky-400" />
              <span>Active Tasks ({s.active_tasks.length})</span>
            </h2>
            <div className="bg-surface border border-surface-border rounded-lg p-3 space-y-2 max-h-60 overflow-y-auto">
              {s.active_tasks.length === 0 ? (
                <div className="text-xs text-slate-400 font-mono text-center py-4">
                  No active tasks executing right now
                </div>
              ) : (
                s.active_tasks.map((task) => (
                  <div
                    key={task.id}
                    onClick={() => onSelectWorkflow(task.workflow_id)}
                    className="p-2 bg-[#151c2e] hover:bg-[#1a233a] border border-surface-border rounded cursor-pointer transition-colors"
                  >
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-xs font-medium text-slate-200 truncate max-w-[180px]">
                        {task.objective}
                      </span>
                      <span
                        className={`text-[9px] font-mono uppercase px-1.5 py-0.2 rounded border ${getStatusColor(
                          task.state
                        )}`}
                      >
                        {task.state}
                      </span>
                    </div>
                    <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                      <span>Agent: {task.assigned_agent_id ? task.assigned_agent_id.slice(0, 8) : 'Unassigned'}</span>
                      <span>Pri: {task.priority}</span>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
