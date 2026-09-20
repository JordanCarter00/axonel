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
import { Button, Badge, BadgeVariant, Panel, Modal, Input, Select, Textarea } from './ui';

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

  const getStatusVariant = (state: string): BadgeVariant => {
    switch (state.toLowerCase()) {
      case 'busy':
        return 'running';
      case 'idle':
        return 'verified';
      case 'paused':
        return 'awaiting';
      case 'terminated':
        return 'failed';
      default:
        return 'neutral';
    }
  };

  return (
    <div className="space-y-5 pb-12">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-2 border-b border-surface-border">
        <div>
          <div className="flex items-center gap-2">
            <Bot className="w-4 h-4 text-axonel-lime" />
            <h1 className="text-base sm:text-lg font-bold text-gray-100 tracking-tight font-mono uppercase">
              Agent Fleet Governance
            </h1>
          </div>
          <p className="text-xs text-gray-400 mt-0.5">
            Registered autonomous agent runtimes, specialized capabilities, and state inspection
          </p>
        </div>

        <Button
          onClick={onRefresh}
          variant="outline"
          size="sm"
          title="Refresh Agents"
          aria-label="Refresh Agents"
          loading={loading}
          icon={<RefreshCw className="w-3.5 h-3.5 text-gray-400" />}
        />
      </div>

      {/* Agents Roster Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {loading && agents.length === 0 ? (
          <div className="col-span-full p-12 text-center text-gray-400 font-mono text-xs">
            Loading agent registry...
          </div>
        ) : agents.length === 0 ? (
          <div className="col-span-full p-12 text-center text-gray-400 text-xs bg-surface-card border border-surface-border rounded">
            No agents registered in runtime.
          </div>
        ) : (
          agents.map((agent) => (
            <Panel
              key={agent.id}
              dense
              className="flex flex-col justify-between"
            >
              <div className="space-y-3">
                <div className="flex items-start justify-between gap-2">
                  <div className="flex items-center gap-2.5">
                    <div className="w-7 h-7 rounded bg-surface-base border border-surface-border flex items-center justify-center text-axonel-lime">
                      <Cpu className="w-3.5 h-3.5" />
                    </div>
                    <div>
                      <h3 className="text-xs sm:text-sm font-semibold text-gray-100">
                        {agent.display_name}
                      </h3>
                      <p className="text-[11px] text-gray-400">{agent.role}</p>
                    </div>
                  </div>
                  <Badge
                    variant={getStatusVariant(agent.state)}
                    size="xs"
                    statusDot
                    pulse={agent.state.toLowerCase() === 'busy'}
                  >
                    {agent.state}
                  </Badge>
                </div>

                <div className="text-[10px] font-mono text-gray-500">ID: {agent.id}</div>

                {/* Capabilities Badges */}
                <div className="space-y-1">
                  <div className="text-[10px] font-semibold text-gray-400 uppercase tracking-wider">
                    Capabilities:
                  </div>
                  <div className="flex flex-wrap gap-1">
                    {agent.capabilities.map((cap) => (
                      <span
                        key={cap}
                        className="text-[10px] font-mono px-1.5 py-0.2 bg-surface-base text-gray-300 rounded border border-surface-border"
                      >
                        {cap}
                      </span>
                    ))}
                  </div>
                </div>

                {/* Current task */}
                {agent.current_task_id && (
                  <div className="p-2 bg-surface-base rounded border border-surface-border text-[11px] font-mono text-status-running">
                    Active Task: {agent.current_task_id.slice(0, 10)}...
                  </div>
                )}
              </div>

              {/* Agent Actions Toolbar */}
              <div className="pt-3 mt-3 border-t border-surface-border flex items-center justify-between gap-2">
                <div className="flex items-center gap-1">
                  {agent.state === 'Busy' && (
                    <Button
                      onClick={() => handlePause(agent.id)}
                      variant="secondary"
                      size="xs"
                      title="Pause Agent"
                      className="px-2"
                    >
                      <Pause className="w-3 h-3 text-amber-400" />
                    </Button>
                  )}
                  {agent.state === 'Paused' && (
                    <Button
                      onClick={() => handleResume(agent.id)}
                      variant="secondary"
                      size="xs"
                      title="Resume Agent"
                      className="px-2"
                    >
                      <Play className="w-3 h-3 text-emerald-400" />
                    </Button>
                  )}
                  {agent.state !== 'Terminated' && (
                    <Button
                      onClick={() => handleCancel(agent.id)}
                      variant="danger-ghost"
                      size="xs"
                      title="Terminate Agent"
                      className="px-2"
                    >
                      <Ban className="w-3 h-3 text-red-400" />
                    </Button>
                  )}
                </div>

                <Button
                  onClick={() => setActiveMessageAgent(agent)}
                  variant="outline"
                  size="xs"
                  icon={<MessageSquare className="w-3 h-3 text-axonel-lime" />}
                >
                  Direct Message
                </Button>
              </div>
            </Panel>
          ))
        )}
      </div>

      {/* Direct Message Modal */}
      {activeMessageAgent && (
        <Modal
          isOpen={Boolean(activeMessageAgent)}
          onClose={() => setActiveMessageAgent(null)}
          title={
            <div className="flex items-center gap-2">
              <MessageSquare className="w-4 h-4 text-axonel-lime" />
              <span>Message to {activeMessageAgent.display_name}</span>
            </div>
          }
          subtitle={`Agent ID: ${activeMessageAgent.id}`}
          maxWidth="md"
          footer={
            <div className="w-full flex items-center justify-end gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setActiveMessageAgent(null)}
              >
                Cancel
              </Button>
              <Button
                disabled={isSending || !messageContent}
                onClick={handleSendMessage}
                variant="primary"
                size="sm"
                loading={isSending}
                icon={<Send className="w-3.5 h-3.5" />}
              >
                Send Message
              </Button>
            </div>
          }
        >
          <div className="space-y-3">
            <Input
              label="Workflow Context (optional)"
              placeholder="Workflow ID..."
              value={targetWorkflowId}
              onChange={(e) => setTargetWorkflowId(e.target.value)}
              mono
            />

            <Select
              label="Message Type"
              value={messageType}
              onChange={(e) => setMessageType(e.target.value)}
              mono
            >
              <option value="Directive">Directive</option>
              <option value="Query">Query</option>
              <option value="Feedback">Feedback</option>
              <option value="Handoff">Handoff</option>
            </Select>

            <Textarea
              label="Instruction / Payload"
              rows={4}
              placeholder="Enter direct operator guidance or data payload..."
              value={messageContent}
              onChange={(e) => setMessageContent(e.target.value)}
              mono
            />
          </div>
        </Modal>
      )}
    </div>
  );
};
