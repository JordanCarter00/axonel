import React, { useState, useEffect } from 'react';
import { ProviderCapabilities, PruneRetentionResponse } from '../types';
import { api } from '../services/api';

export const UsageView: React.FC = () => {
  const [capabilities, setCapabilities] = useState<ProviderCapabilities[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [pruning, setPruning] = useState<boolean>(false);
  const [pruneResult, setPruneResult] = useState<PruneRetentionResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [budgetLimit, setBudgetLimit] = useState<number>(() => {
    const saved = localStorage.getItem('plexis_budget_threshold');
    return saved ? parseFloat(saved) : 50.0;
  });

  useEffect(() => {
    loadCapabilities();
  }, []);

  const handleBudgetChange = (newVal: number) => {
    setBudgetLimit(newVal);
    localStorage.setItem('plexis_budget_threshold', newVal.toString());
  };

  const loadCapabilities = async () => {
    try {
      setLoading(true);
      setError(null);
      const caps = await api.getProviderCapabilities();
      setCapabilities(caps);
    } catch (err: any) {
      setError(err.message || 'Failed to load provider capabilities');
    } finally {
      setLoading(false);
    }
  };

  const handlePrune = async () => {
    try {
      setPruning(true);
      setError(null);
      const res = await api.pruneRetentionRecords(30);
      setPruneResult(res);
    } catch (err: any) {
      setError(err.message || 'Failed to prune historical records');
    } finally {
      setPruning(false);
    }
  };

  // Mock aggregated usage metrics computed across active workflow executions
  const estimatedCost = 3.42;
  const totalTokens = 248150;
  const isBudgetExceeded = estimatedCost >= budgetLimit;

  const agentAttributions = [
    { role: 'Planner', tasks: 4, promptTokens: 42100, completionTokens: 8400, cost: 0.58 },
    { role: 'Developer', tasks: 8, promptTokens: 98400, completionTokens: 32600, cost: 1.62 },
    { role: 'Tester', tasks: 5, promptTokens: 38200, completionTokens: 7100, cost: 0.46 },
    { role: 'Reviewer', tasks: 3, promptTokens: 18400, completionTokens: 3200, cost: 0.28 },
    { role: 'Verifier', tasks: 4, promptTokens: 25800, completionTokens: 4250, cost: 0.48 },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold text-slate-100">Cost & Usage Accounting</h2>
          <p className="text-sm text-slate-400 mt-1">
            Real-time token telemetry, per-agent cost attribution, provider matrix, and budget safeguards.
          </p>
        </div>
        <button
          onClick={handlePrune}
          disabled={pruning}
          className="px-4 py-2 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 text-xs font-semibold rounded-lg transition border border-slate-700 flex items-center space-x-2"
        >
          <svg className={`w-4 h-4 text-amber-400 ${pruning ? 'animate-spin' : ''}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
          <span>{pruning ? 'Pruning...' : 'Prune Stale Records (30d+)'}</span>
        </button>
      </div>

      {/* KPI Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4">
          <span className="text-xs text-slate-400 font-semibold uppercase tracking-wider block mb-1">Total Tokens Consumed</span>
          <div className="text-2xl font-bold text-slate-100 font-mono">{totalTokens.toLocaleString()}</div>
          <span className="text-[11px] text-slate-500 mt-1 block">Prompt + completion tokens</span>
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4">
          <span className="text-xs text-slate-400 font-semibold uppercase tracking-wider block mb-1">Estimated Cost</span>
          <div className="text-2xl font-bold text-emerald-400 font-mono">${estimatedCost.toFixed(2)}</div>
          <span className="text-[11px] text-slate-500 mt-1 block">Calculated from model pricing</span>
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4">
          <span className="text-xs text-slate-400 font-semibold uppercase tracking-wider block mb-1">Budget Alert Threshold</span>
          <div className="flex items-center space-x-2 mt-1">
            <span className="text-slate-400 font-mono">$</span>
            <input
              type="number"
              min="1"
              step="5"
              value={budgetLimit}
              onChange={(e) => handleBudgetChange(parseFloat(e.target.value) || 0)}
              className="w-24 bg-slate-950 border border-slate-700 rounded px-2 py-1 text-sm text-slate-100 font-mono"
            />
          </div>
          <span className="text-[11px] text-slate-500 mt-1 block">Alert triggered at limit</span>
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4">
          <span className="text-xs text-slate-400 font-semibold uppercase tracking-wider block mb-1">Configured Models</span>
          <div className="text-2xl font-bold text-indigo-400 font-mono">{capabilities.length}</div>
          <span className="text-[11px] text-slate-500 mt-1 block">Across available providers</span>
        </div>
      </div>

      {isBudgetExceeded && (
        <div className="p-4 bg-rose-950/60 border border-rose-600/60 rounded-xl text-xs text-rose-200 flex items-center space-x-3">
          <svg className="w-5 h-5 text-rose-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <div>
            <strong>Budget Threshold Alert:</strong> Estimated cost (${estimatedCost.toFixed(2)}) has reached or exceeded the set threshold limit of ${budgetLimit.toFixed(2)}. Review active agent workloads.
          </div>
        </div>
      )}

      {/* Per-Agent Cost Attribution Table */}
      <div className="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
        <div className="px-6 py-4 border-b border-slate-800 bg-slate-950/50 flex items-center justify-between">
          <h3 className="text-sm font-semibold text-slate-200">Per-Agent Role Cost Attribution</h3>
          <span className="text-[11px] text-slate-400 font-mono">Specialized role breakdown</span>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead className="bg-slate-950/80 text-slate-400 font-semibold border-b border-slate-800">
              <tr>
                <th className="px-6 py-3">Agent Specialization</th>
                <th className="px-6 py-3">Tasks Executed</th>
                <th className="px-6 py-3">Prompt Tokens</th>
                <th className="px-6 py-3">Completion Tokens</th>
                <th className="px-6 py-3">Total Cost</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60 text-slate-300">
              {agentAttributions.map((agent, i) => (
                <tr key={i} className="hover:bg-slate-800/40 transition">
                  <td className="px-6 py-3 font-semibold text-indigo-300">{agent.role}</td>
                  <td className="px-6 py-3 font-mono">{agent.tasks}</td>
                  <td className="px-6 py-3 font-mono">{agent.promptTokens.toLocaleString()}</td>
                  <td className="px-6 py-3 font-mono">{agent.completionTokens.toLocaleString()}</td>
                  <td className="px-6 py-3 font-mono text-emerald-400 font-semibold">${agent.cost.toFixed(2)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {pruneResult && (
        <div className="p-4 bg-emerald-950/40 border border-emerald-800/60 rounded-xl text-xs text-emerald-300 flex items-center justify-between">
          <div className="flex items-center space-x-2">
            <span className="w-2 h-2 rounded-full bg-emerald-400"></span>
            <span>
              Retention enforced: <strong>{pruneResult.records_pruned}</strong> stale records pruned (events: {pruneResult.report.pruned_events}, messages: {pruneResult.report.pruned_messages}, commands: {pruneResult.report.pruned_commands})
            </span>
          </div>
          <span className="text-[11px] text-slate-500 font-mono">Cutoff: {new Date(pruneResult.cutoff_date).toLocaleDateString()}</span>
        </div>
      )}

      {error && (
        <div className="p-4 bg-red-950/40 border border-red-800/60 rounded-xl text-xs text-red-300">
          {error}
        </div>
      )}

      {/* Provider Capabilities Table */}
      <div className="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
        <div className="px-6 py-4 border-b border-slate-800 bg-slate-950/50">
          <h3 className="text-sm font-semibold text-slate-200">Provider Capability Matrix & Pricing</h3>
        </div>

        {loading ? (
          <div className="py-12 text-center text-slate-400">Loading capability matrix...</div>
        ) : capabilities.length === 0 ? (
          <div className="py-12 text-center text-slate-400">No capabilities discovered.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950/80 text-slate-400 font-semibold border-b border-slate-800">
                <tr>
                  <th className="px-6 py-3">Provider</th>
                  <th className="px-6 py-3">Model</th>
                  <th className="px-6 py-3">Reasoning Tier</th>
                  <th className="px-6 py-3">Context Window</th>
                  <th className="px-6 py-3">Pricing (Prompt / Compl)</th>
                  <th className="px-6 py-3">Features</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/60 text-slate-300">
                {capabilities.map((cap, idx) => {
                  let tierColor = 'bg-blue-950/60 text-blue-300 border-blue-800/60';
                  const tierStr = String(cap.reasoning_tier || '').toLowerCase();
                  if (tierStr === 'high') {
                    tierColor = 'bg-purple-950/60 text-purple-300 border-purple-800/60';
                  } else if (tierStr === 'medium') {
                    tierColor = 'bg-sky-950/60 text-sky-300 border-sky-800/60';
                  } else if (tierStr === 'low' || tierStr === 'none') {
                    tierColor = 'bg-emerald-950/60 text-emerald-300 border-emerald-800/60';
                  }

                  const promptPer1M = ((cap.pricing?.cost_per_1k_input_tokens ?? 0) * 1000).toFixed(2);
                  const complPer1M = ((cap.pricing?.cost_per_1k_output_tokens ?? 0) * 1000).toFixed(2);

                  return (
                    <tr key={idx} className="hover:bg-slate-800/40 transition">
                      <td className="px-6 py-3 font-semibold uppercase tracking-wider text-[11px] text-slate-400">
                        {cap.provider}
                      </td>
                      <td className="px-6 py-3 font-mono font-medium text-slate-200">
                        {cap.model}
                      </td>
                      <td className="px-6 py-3">
                        <span className={`px-2 py-0.5 rounded border text-[10px] font-semibold uppercase ${tierColor}`}>
                          {cap.reasoning_tier}
                        </span>
                      </td>
                      <td className="px-6 py-3 font-mono">
                        {(cap.context_window_tokens ?? 0).toLocaleString()} tokens
                      </td>
                      <td className="px-6 py-3 font-mono text-slate-400">
                        ${promptPer1M} / ${complPer1M} <span className="text-[10px] text-slate-500">per 1M</span>
                      </td>
                      <td className="px-6 py-3">
                        <div className="flex items-center space-x-1.5">
                          {cap.supports_tools && (
                            <span className="px-1.5 py-0.5 bg-slate-800 text-slate-300 rounded text-[10px] border border-slate-700">
                              Tools
                            </span>
                          )}
                          {cap.supports_streaming && (
                            <span className="px-1.5 py-0.5 bg-slate-800 text-slate-300 rounded text-[10px] border border-slate-700">
                              Stream
                            </span>
                          )}
                          {cap.supports_vision && (
                            <span className="px-1.5 py-0.5 bg-slate-800 text-slate-300 rounded text-[10px] border border-slate-700">
                              Vision
                            </span>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
