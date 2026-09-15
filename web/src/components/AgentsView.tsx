import React, { useState } from 'react';
import { Agent } from '../types';
import { api } from '../services/api';
import {
  Bot,
  Pause,
  Play,
  Ban,
  MessageSquare,
  Cpu,
  Send,
  RefreshCw,
} from 'lucide-react';

interface AgentsViewProps {
  agents: Agent[];
  loading: boolean;
  onRefresh: () => void;
  onSelectWorkflow: (id: string) => void;
}

export const AgentsView: React.FC<AgentsViewProps> = ({
  agents,
  loading,
  onRefresh,
}) => {
  const [activeMessageAgent, setActiveMessageAgent] = useState<Agent | null>(null);
  const [targetWorkflowId, setTargetWorkflowId] = useState('');
  const [messageType, setMessageType] = useState('Directive');
  const [messageContent, setMessageContent] = useState('');
  const [isSending, setIsSending] = useState(false);

  const handlePause = async (id: string) => {
    try {
      await api.pauseAgent(id);
      onRefresh();
    } catch (e) {
      alert(`Pause agent failed: ${e}`);
    }
  };

  const handleResume = async (id: string) => {
    try {
      await api.resumeAgent(id);
      onRefresh();
    } catch (e) {
      alert(`Resume agent failed: ${e}`);
    }
  };

  const handleCancel = async (id: string) => {
    if (!confirm('Are you sure you want to terminate this agent?')) return;
    try {
      await api.cancelAgent(id);
      onRefresh();
    } catch (e) {
      alert(`Terminate agent failed: ${e}`);
    }
  };

  const handleSendMessage = async () => {
    if (!activeMessageAgent || !messageContent) return;
    setIsSending(true);
    try {
      await api.sendAgentMessage(activeMessageAgent.id, {
        to_agent: activeMessageAgent.id,
        workflow_id: targetWorkflowId,
        message_type: messageType,
        content: messageContent,
      });
      setActiveMessageAgent(null);
      setMessageContent('');
      setTargetWorkflowId('');
      alert('Message dispatched to agent communication channel.');
    } catch (e) {
      alert(`Send message failed: ${e}`);
    } finally {
      setIsSending(false);
    }
  };

  const getStatusBadge = (state: string) => {
    switch (state.toLowerCase()) {
      case 'busy':
        return 'bg-sky-500/10 text-sky-400 border-sky-500/30 animate-pulse';
      case 'idle':
        return 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
      case 'paused':
        return 'bg-amber-500/10 text-amber-400 border-amber-500/30';
      case 'terminated':
        return 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      default:
        return 'bg-slate-800 text-slate-400 border-slate-700';
    }
  };

  return (
    <div className="space-y-6 pb-12">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <Bot className="w-5 h-5 text-purple-400" />
            <span>Multi-Agent Fleet Governance</span>
          </h1>
          <p className="text-xs text-slate-400">
            Specialized autonomous agents, role capabilities, and execution state
          </p>
        </div>

        <button
          onClick={onRefresh}
          className="p-2 text-slate-400 hover:text-slate-200 bg-surface border border-surface-border rounded-md"
          title="Refresh Agents"
        >
          <RefreshCw className="w-4 h-4" />
        </button>
      </div>

      {/* Agents Roster Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {loading && agents.length === 0 ? (
          <div className="col-span-full p-12 text-center text-slate-400 font-mono text-xs">
            Loading agent registry...
          </div>
        ) : agents.length === 0 ? (
          <div className="col-span-full p-12 text-center text-slate-400">
            No agents registered in runtime.
          </div>
        ) : (
          agents.map((agent) => (
            <div
              key={agent.id}
              className="bg-surface border border-surface-border rounded-lg p-4 space-y-3 flex flex-col justify-between"
            >
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <div className="w-7 h-7 rounded bg-purple-500/20 text-purple-400 flex items-center justify-center font-bold text-xs">
                      <Cpu className="w-3.5 h-3.5" />
                    </div>
                    <div>
                      <h3 className="text-sm font-semibold text-slate-200">{agent.display_name}</h3>
                      <p className="text-[11px] text-slate-400">{agent.role}</p>
                    </div>
                  </div>
                  <span
                    className={`text-[10px] font-mono uppercase px-2 py-0.5 rounded border ${getStatusBadge(
                      agent.state
                    )}`}
                  >
                    {agent.state}
                  </span>
                </div>

                <div className="text-[10px] font-mono text-slate-500">ID: {agent.id}</div>

                {/* Capabilities Badges */}
                <div className="space-y-1">
                  <div className="text-[10px] font-semibold text-slate-400 uppercase tracking-wider">
                    Capabilities:
                  </div>
                  <div className="flex flex-wrap gap-1">
                    {agent.capabilities.map((cap) => (
                      <span
                        key={cap}
                        className="text-[10px] font-mono px-1.5 py-0.2 bg-[#0a0d14] text-slate-300 rounded border border-surface-border"
                      >
                        {cap}
                      </span>
                    ))}
                  </div>
                </div>

                {/* Current task or active session */}
                {agent.current_task_id && (
                  <div className="p-2 bg-[#0a0d14] rounded text-[11px] font-mono text-sky-300">
                    Executing Task: {agent.current_task_id.slice(0, 10)}...
                  </div>
                )}
              </div>

              {/* Agent Actions Toolbar */}
              <div className="pt-3 border-t border-surface-border flex items-center justify-between">
                <div className="flex items-center space-x-1">
                  {agent.state === 'Busy' && (
                    <button
                      onClick={() => handlePause(agent.id)}
                      className="p-1.5 rounded hover:bg-surface-hover text-amber-400 border border-surface-border"
                      title="Pause Agent"
                    >
                      <Pause className="w-3.5 h-3.5" />
                    </button>
                  )}
                  {agent.state === 'Paused' && (
                    <button
                      onClick={() => handleResume(agent.id)}
                      className="p-1.5 rounded hover:bg-surface-hover text-emerald-400 border border-surface-border"
                      title="Resume Agent"
                    >
                      <Play className="w-3.5 h-3.5" />
                    </button>
                  )}
                  {agent.state !== 'Terminated' && (
                    <button
                      onClick={() => handleCancel(agent.id)}
                      className="p-1.5 rounded hover:bg-surface-hover text-rose-400 border border-surface-border"
                      title="Terminate Agent"
                    >
                      <Ban className="w-3.5 h-3.5" />
                    </button>
                  )}
                </div>

                <button
                  onClick={() => setActiveMessageAgent(agent)}
                  className="flex items-center space-x-1 px-2.5 py-1 bg-surface-border hover:bg-surface-hover rounded text-xs text-slate-200"
                >
                  <MessageSquare className="w-3 h-3 text-indigo-400" />
                  <span>Direct Message</span>
                </button>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Direct Message Modal */}
      {activeMessageAgent && (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4">
          <div className="bg-surface border border-surface-border rounded-lg max-w-md w-full p-5 space-y-4">
            <div className="flex items-center justify-between border-b border-surface-border pb-3">
              <h3 className="text-sm font-semibold text-slate-100 flex items-center space-x-2">
                <MessageSquare className="w-4 h-4 text-indigo-400" />
                <span>Message to {activeMessageAgent.display_name}</span>
              </h3>
              <button
                onClick={() => setActiveMessageAgent(null)}
                className="text-slate-400 hover:text-white text-xs"
              >
                Cancel
              </button>
            </div>

            <div className="space-y-3">
              <div>
                <label className="block text-xs text-slate-400 mb-1">Workflow Context (optional)</label>
                <input
                  type="text"
                  placeholder="Workflow ID..."
                  value={targetWorkflowId}
                  onChange={(e) => setTargetWorkflowId(e.target.value)}
                  className="w-full bg-[#0a0d14] border border-surface-border rounded px-3 py-1.5 text-xs text-slate-200"
                />
              </div>

              <div>
                <label className="block text-xs text-slate-400 mb-1">Message Type</label>
                <select
                  value={messageType}
                  onChange={(e) => setMessageType(e.target.value)}
                  className="w-full bg-[#0a0d14] border border-surface-border rounded px-3 py-1.5 text-xs text-slate-200"
                >
                  <option value="Directive">Directive</option>
                  <option value="Query">Query</option>
                  <option value="Feedback">Feedback</option>
                  <option value="Handoff">Handoff</option>
                </select>
              </div>

              <div>
                <label className="block text-xs text-slate-400 mb-1">Instruction / Payload</label>
                <textarea
                  rows={4}
                  placeholder="Enter direct operator guidance or data payload..."
                  value={messageContent}
                  onChange={(e) => setMessageContent(e.target.value)}
                  className="w-full bg-[#0a0d14] border border-surface-border rounded p-2.5 text-xs text-slate-200 font-mono"
                />
              </div>
            </div>

            <div className="flex items-center justify-end space-x-2 pt-2 border-t border-surface-border">
              <button
                onClick={() => setActiveMessageAgent(null)}
                className="px-3 py-1.5 text-slate-400 hover:text-slate-200 text-xs"
              >
                Cancel
              </button>
              <button
                disabled={isSending || !messageContent}
                onClick={handleSendMessage}
                className="flex items-center space-x-1.5 px-3 py-1.5 bg-primary-600 hover:bg-primary-500 disabled:opacity-50 text-white rounded text-xs font-semibold"
              >
                <Send className="w-3.5 h-3.5" />
                <span>Send Message</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
