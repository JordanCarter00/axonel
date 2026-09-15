import React, { useState } from 'react';
import { ApprovalRecord } from '../types';
import { api } from '../services/api';
import {
  ShieldAlert,
  ShieldCheck,
  CheckCircle,
  XCircle,
  Clock,
  ExternalLink,
  RefreshCw,
} from 'lucide-react';

interface ApprovalsViewProps {
  approvals: ApprovalRecord[];
  loading: boolean;
  onRefresh: () => void;
  onSelectWorkflow: (id: string) => void;
}

export const ApprovalsView: React.FC<ApprovalsViewProps> = ({
  approvals,
  loading,
  onRefresh,
  onSelectWorkflow,
}) => {
  const [filter, setFilter] = useState<'pending' | 'approved' | 'rejected' | 'all'>('pending');
  const [activeActionId, setActiveActionId] = useState<string | null>(null);
  const [decisionNotes, setDecisionNotes] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const filtered = approvals.filter((a) => {
    if (filter === 'all') return true;
    return a.status.toLowerCase() === filter.toLowerCase();
  });

  const handleApprove = async (id: string) => {
    setIsSubmitting(true);
    try {
      await api.approve(id, decisionNotes || undefined);
      setActiveActionId(null);
      setDecisionNotes('');
      onRefresh();
    } catch (e) {
      alert(`Approval failed: ${e}`);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleReject = async (id: string) => {
    setIsSubmitting(true);
    try {
      await api.reject(id, decisionNotes || 'Rejected by operator');
      setActiveActionId(null);
      setDecisionNotes('');
      onRefresh();
    } catch (e) {
      alert(`Rejection failed: ${e}`);
    } finally {
      setIsSubmitting(false);
    }
  };

  const getRiskBadge = (risk: string) => {
    switch (risk.toLowerCase()) {
      case 'critical':
        return 'bg-rose-950/60 text-rose-300 border-rose-500/60 animate-pulse';
      case 'high':
        return 'bg-amber-950/60 text-amber-300 border-amber-500/60';
      case 'medium':
        return 'bg-yellow-950/40 text-yellow-300 border-yellow-500/40';
      default:
        return 'bg-sky-950/40 text-sky-300 border-sky-500/40';
    }
  };

  return (
    <div className="space-y-6 pb-12">
      {/* Top Banner & Title */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-slate-100 flex items-center space-x-2">
            <ShieldAlert className="w-5 h-5 text-amber-400" />
            <span>Human Governance & Approval Center</span>
          </h1>
          <p className="text-xs text-slate-400">
            Authoritative intervention and policy verification for high-risk autonomous actions
          </p>
        </div>

        <div className="flex items-center space-x-2">
          <button
            onClick={onRefresh}
            className="p-2 text-slate-400 hover:text-slate-200 bg-surface border border-surface-border rounded-md"
            title="Refresh Approvals"
          >
            <RefreshCw className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Filter Tabs */}
      <div className="flex items-center space-x-2 border-b border-surface-border pb-3">
        {(['pending', 'approved', 'rejected', 'all'] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setFilter(tab)}
            className={`px-3 py-1.5 rounded-md text-xs font-semibold capitalize transition-colors ${
              filter === tab
                ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            {tab}
            {tab === 'pending' && approvals.filter((a) => a.status === 'Pending').length > 0 && (
              <span className="ml-1.5 px-1.5 py-0.2 rounded-full bg-amber-500/20 text-amber-300 text-[10px]">
                {approvals.filter((a) => a.status === 'Pending').length}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Approvals List */}
      <div className="space-y-4">
        {loading && approvals.length === 0 ? (
          <div className="p-12 text-center text-slate-400 font-mono text-xs">
            Loading approval requests...
          </div>
        ) : filtered.length === 0 ? (
          <div className="bg-surface border border-surface-border rounded-lg p-12 text-center text-slate-400">
            <ShieldCheck className="w-8 h-8 mx-auto text-emerald-400 mb-2 opacity-80" />
            <p className="text-sm font-semibold text-slate-300">No {filter} approvals</p>
            <p className="text-xs text-slate-400 mt-1">All autonomous workflows operating nominal.</p>
          </div>
        ) : (
          filtered.map((approval) => (
            <div
              key={approval.id}
              className="bg-surface border border-surface-border rounded-lg p-5 space-y-4"
            >
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 border-b border-surface-border pb-3">
                <div className="flex items-center space-x-3">
                  <span
                    className={`text-xs font-mono font-bold uppercase px-2.5 py-1 rounded border ${getRiskBadge(
                      approval.risk_level
                    )}`}
                  >
                    {approval.risk_level} RISK
                  </span>
                  <span className="text-sm font-bold text-slate-200">{approval.action_name}</span>
                </div>

                <div className="flex items-center space-x-4 text-xs font-mono text-slate-400">
                  <div
                    onClick={() => onSelectWorkflow(approval.workflow_id)}
                    className="flex items-center space-x-1 text-primary-400 hover:text-primary-300 cursor-pointer"
                  >
                    <span>Workflow: {approval.workflow_id.slice(0, 8)}...</span>
                    <ExternalLink className="w-3 h-3" />
                  </div>
                  <div className="flex items-center space-x-1 text-slate-500">
                    <Clock className="w-3 h-3" />
                    <span>{new Date(approval.requested_at).toLocaleString()}</span>
                  </div>
                </div>
              </div>

              {/* Action Payload Details */}
              <div className="space-y-1.5">
                <div className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
                  Action Parameters & Security Context
                </div>
                <div className="bg-[#0a0d14] p-3 rounded-md font-mono text-xs text-slate-300 overflow-x-auto max-h-48 border border-surface-border">
                  <pre>{JSON.stringify(approval.details, null, 2)}</pre>
                </div>
              </div>

              {/* Status or Decision Details */}
              {approval.status !== 'Pending' && (
                <div className="p-3 bg-[#131b2e] rounded border border-surface-border flex items-center justify-between text-xs">
                  <div>
                    <span className="text-slate-400">Decided by: </span>
                    <span className="text-slate-200 font-mono">{approval.approver || 'operator'}</span>
                    {approval.reason && (
                      <span className="text-slate-400"> — Reason: "{approval.reason}"</span>
                    )}
                  </div>
                  <span
                    className={`font-mono uppercase font-bold text-[10px] px-2 py-0.5 rounded border ${
                      approval.status === 'Approved'
                        ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                        : 'bg-rose-500/10 text-rose-400 border-rose-500/30'
                    }`}
                  >
                    {approval.status}
                  </span>
                </div>
              )}

              {/* Operator Decision Controls (if Pending) */}
              {approval.status === 'Pending' && (
                <div className="space-y-3 pt-2">
                  {activeActionId === approval.id ? (
                    <div className="p-3 bg-[#0e1424] rounded-lg border border-indigo-500/30 space-y-3">
                      <label className="block text-xs font-medium text-slate-300">
                        Operator Notes / Rationale (optional for approval, required for audit)
                      </label>
                      <input
                        type="text"
                        placeholder="e.g. Verified sandbox bounds, approved for staging execution..."
                        value={decisionNotes}
                        onChange={(e) => setDecisionNotes(e.target.value)}
                        className="w-full bg-[#0a0d14] border border-surface-border rounded px-3 py-1.5 text-xs text-slate-200"
                      />
                      <div className="flex items-center justify-end space-x-2">
                        <button
                          onClick={() => {
                            setActiveActionId(null);
                            setDecisionNotes('');
                          }}
                          className="px-3 py-1.5 text-slate-400 hover:text-slate-200 text-xs"
                        >
                          Cancel
                        </button>
                        <button
                          disabled={isSubmitting}
                          onClick={() => handleReject(approval.id)}
                          className="px-3 py-1.5 bg-rose-600 hover:bg-rose-500 text-white rounded text-xs font-semibold flex items-center space-x-1"
                        >
                          <XCircle className="w-3.5 h-3.5" />
                          <span>Confirm Reject</span>
                        </button>
                        <button
                          disabled={isSubmitting}
                          onClick={() => handleApprove(approval.id)}
                          className="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold flex items-center space-x-1"
                        >
                          <CheckCircle className="w-3.5 h-3.5" />
                          <span>Confirm Approve</span>
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-center justify-end space-x-2">
                      <button
                        onClick={() => {
                          setActiveActionId(approval.id);
                          setDecisionNotes('');
                        }}
                        className="px-3.5 py-1.5 bg-surface-border hover:bg-surface-hover text-slate-200 rounded text-xs font-medium transition-colors"
                      >
                        Review Decision...
                      </button>
                      <button
                        disabled={isSubmitting}
                        onClick={() => handleApprove(approval.id)}
                        className="px-3.5 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-semibold flex items-center space-x-1.5 transition-colors shadow-sm shadow-emerald-600/20"
                      >
                        <CheckCircle className="w-3.5 h-3.5" />
                        <span>Quick Approve</span>
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
};
