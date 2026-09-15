import React from 'react';
import {
  Activity,
  Layers,
  ShieldCheck,
  Radio,
  Bot,
  Database,
  Wrench,
  Server,
  Plus,
  Settings,
  RefreshCw,
} from 'lucide-react';
import { ConnectionState } from '../services/sse';

export type TabType =
  | 'dashboard'
  | 'workflows'
  | 'approvals'
  | 'timeline'
  | 'agents'
  | 'memory'
  | 'tools'
  | 'providers';

interface HeaderProps {
  activeTab: TabType;
  onSelectTab: (tab: TabType) => void;
  connectionState: ConnectionState;
  cursor: number;
  pendingApprovalsCount: number;
  onOpenNewWorkflow: () => void;
  onOpenSettings: () => void;
  onRefresh: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  activeTab,
  onSelectTab,
  connectionState,
  cursor,
  pendingApprovalsCount,
  onOpenNewWorkflow,
  onOpenSettings,
  onRefresh,
}) => {
  return (
    <header className="bg-[#0e1320] border-b border-surface-border sticky top-0 z-40 px-4 py-2.5">
      <div className="max-w-7xl mx-auto flex items-center justify-between">
        {/* Brand */}
        <div className="flex items-center space-x-6">
          <div className="flex items-center space-x-2.5 cursor-pointer" onClick={() => onSelectTab('dashboard')}>
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-primary-700 flex items-center justify-center shadow-lg shadow-indigo-500/20">
              <span className="font-mono font-bold text-white text-lg leading-none">P</span>
            </div>
            <div>
              <div className="flex items-center space-x-1.5">
                <span className="font-semibold text-slate-100 tracking-tight text-sm">PLEXIS</span>
                <span className="text-[10px] font-mono uppercase tracking-wider px-1.5 py-0.5 rounded bg-surface-border text-slate-400">
                  Control Plane
                </span>
              </div>
              <p className="text-[11px] text-slate-400 font-mono">autonomous runtime</p>
            </div>
          </div>

          {/* Navigation Tabs */}
          <nav className="hidden md:flex items-center space-x-1">
            <button
              onClick={() => onSelectTab('dashboard')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                activeTab === 'dashboard'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <Activity className="w-3.5 h-3.5" />
              <span>Dashboard</span>
            </button>

            <button
              onClick={() => onSelectTab('workflows')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                activeTab === 'workflows'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <Layers className="w-3.5 h-3.5" />
              <span>Workflows</span>
            </button>

            <button
              onClick={() => onSelectTab('approvals')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors relative ${
                activeTab === 'approvals'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <ShieldCheck className="w-3.5 h-3.5" />
              <span>Approvals</span>
              {pendingApprovalsCount > 0 && (
                <span className="ml-1 px-1.5 py-0.2 bg-amber-500/20 text-amber-400 border border-amber-500/40 rounded-full text-[10px] font-mono font-bold animate-pulse">
                  {pendingApprovalsCount}
                </span>
              )}
            </button>

            <button
              onClick={() => onSelectTab('timeline')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                activeTab === 'timeline'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <Radio className="w-3.5 h-3.5" />
              <span>Live Timeline</span>
            </button>

            <button
              onClick={() => onSelectTab('agents')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                activeTab === 'agents'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <Bot className="w-3.5 h-3.5" />
              <span>Agents</span>
            </button>

            <button
              onClick={() => onSelectTab('memory')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                activeTab === 'memory'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <Database className="w-3.5 h-3.5" />
              <span>Memory</span>
            </button>

            <button
              onClick={() => onSelectTab('tools')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                activeTab === 'tools'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <Wrench className="w-3.5 h-3.5" />
              <span>Tools</span>
            </button>

            <button
              onClick={() => onSelectTab('providers')}
              className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                activeTab === 'providers'
                  ? 'bg-primary-600/20 text-indigo-300 border border-primary-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-surface-hover'
              }`}
            >
              <Server className="w-3.5 h-3.5" />
              <span>Providers</span>
            </button>
          </nav>
        </div>

        {/* Right side controls */}
        <div className="flex items-center space-x-3">
          {/* Stream Connection Pill */}
          <div className="flex items-center space-x-2 px-2.5 py-1 rounded bg-surface border border-surface-border text-xs font-mono">
            <span
              className={`w-2 h-2 rounded-full ${
                connectionState === 'connected'
                  ? 'bg-emerald-400 shadow-sm shadow-emerald-400/50 animate-pulse'
                  : connectionState === 'reconnecting'
                  ? 'bg-amber-400 animate-ping'
                  : 'bg-rose-500'
              }`}
            />
            <span className="capitalize text-slate-300 text-[11px]">{connectionState}</span>
            <span className="text-slate-500 text-[10px]">|</span>
            <span className="text-slate-400 text-[11px]">seq #{cursor}</span>
          </div>

          {/* Refresh button */}
          <button
            onClick={onRefresh}
            title="Refresh authoritative state"
            className="p-1.5 text-slate-400 hover:text-slate-200 hover:bg-surface-hover rounded border border-surface-border transition-colors"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>

          {/* New Workflow button */}
          <button
            onClick={onOpenNewWorkflow}
            className="flex items-center space-x-1.5 px-3 py-1.5 bg-primary-600 hover:bg-primary-500 text-white rounded-md text-xs font-medium shadow-md shadow-indigo-600/20 transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>New Workflow</span>
          </button>

          {/* Settings */}
          <button
            onClick={onOpenSettings}
            className="p-1.5 text-slate-400 hover:text-slate-200 hover:bg-surface-hover rounded border border-surface-border transition-colors"
            title="Settings & Auth"
          >
            <Settings className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </header>
  );
};
