import React, { useEffect, useState } from 'react';
import { useUiStore } from '../store/uiStore';
import { api } from '../api/client';
import type { ConfigResponse } from '../api/types';
import { Cpu, Radio, HardDrive, CloudArrowDown, Download } from '@phosphor-icons/react';

export const SettingsPage: React.FC = () => {
  const { addNotification } = useUiStore();

  const [activeSettingsTab, setActiveSettingsTab] = useState<'models' | 'broadcast' | 'storage'>('models');
  const [extractionModel, setExtractionModel] = useState('mistral-7b-instruct');
  const [narrativeModel, setNarrativeModel] = useState('mixtral-8x7b-instruct');
  const [liveConfig, setLiveConfig] = useState<ConfigResponse | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  
  // Broadcast schedulers
  const [briefingInterval, setBriefingInterval] = useState('hourly');
  const [breakingEnabled, setBreakingEnabled] = useState(true);

  useEffect(() => {
    api
      .getConfig()
      .then((c) => setLiveConfig(c))
      .catch((err: unknown) => setConfigError((err as Error).message ?? 'unavailable'));
  }, []);

  const handleSave = (e: React.FormEvent) => {
    e.preventDefault();
    addNotification('System configuration updated and reloaded.', 'success');
  };

  return (
    <div className="settings-container animate-fade-in">
      <div className="settings-layout">
        {/* Left Side Navigation */}
        <div className="settings-nav">
          <button
            onClick={() => setActiveSettingsTab('models')}
            className={`settings-nav-btn ${activeSettingsTab === 'models' ? 'active' : ''}`}
          >
            <Cpu size={16} />
            Model Registry
          </button>
          
          <button
            onClick={() => setActiveSettingsTab('broadcast')}
            className={`settings-nav-btn ${activeSettingsTab === 'broadcast' ? 'active' : ''}`}
          >
            <Radio size={16} />
            Broadcast Engine
          </button>

          <button
            onClick={() => setActiveSettingsTab('storage')}
            className={`settings-nav-btn ${activeSettingsTab === 'storage' ? 'active' : ''}`}
          >
            <HardDrive size={16} />
            Data & Storage
          </button>
        </div>

        {/* Right Side Content Panel */}
        <div className="settings-content-panel">
          <form onSubmit={handleSave} className="text-left">
            
            {/* Models Tab */}
            {activeSettingsTab === 'models' && (
              <div className="settings-section">
                <h3 className="heading-md mb-3">Local AI Models</h3>
                <p className="text-muted mb-4">
                  Objective runs specialist models locally via llama.cpp and ONNX. Assign specialists for extraction, contradiction, and narrative tasks.
                </p>

                <div className="settings-form-grid">
                  <div className="form-group">
                    <label>Extraction Specialist (3B-7B)</label>
                    <select
                      value={extractionModel}
                      onChange={(e) => setExtractionModel(e.target.value)}
                      className="form-input"
                    >
                      <option value="mistral-7b-instruct">Mistral 7B Instruct (Recommended)</option>
                      <option value="llama3-8b-instruct">Llama 3 8B Instruct</option>
                      <option value="phi3-3.8b-mini">Phi-3 Mini 3.8B</option>
                    </select>
                  </div>

                  <div className="form-group">
                    <label>Narrative & Synthesis Specialist (14B-32B)</label>
                    <select
                      value={narrativeModel}
                      onChange={(e) => setNarrativeModel(e.target.value)}
                      className="form-input"
                    >
                      <option value="mixtral-8x7b-instruct">Mixtral 8x7B (Recommended)</option>
                      <option value="gemma-27b-it">Gemma 2 27B Instruct</option>
                      <option value="command-r-plus">Command R+</option>
                    </select>
                  </div>

                  <div className="model-download-status">
                    <div className="download-info">
                      <CloudArrowDown size={20} className="text-purple" />
                      <div>
                        <span className="download-title">Mistral 7B GGUF</span>
                        <span className="download-desc">Downloaded (4.1 GB)</span>
                      </div>
                    </div>
                    <span className="badge badge-success">ACTIVE</span>
                  </div>
                </div>
              </div>
            )}

            {/* Broadcast Tab */}
            {activeSettingsTab === 'broadcast' && (
              <div className="settings-section">
                <h3 className="heading-md mb-3">Broadcast Engine Schedule</h3>
                <p className="text-muted mb-4">
                  Define the trigger loops for news synthesis, text briefing compile, and TTS podcast recording.
                </p>

                <div className="settings-form-grid">
                  <div className="form-group">
                    <label>Briefing Compile Interval</label>
                    <select
                      value={briefingInterval}
                      onChange={(e) => setBriefingInterval(e.target.value)}
                      className="form-input"
                    >
                      <option value="hourly">Every Hour (0 * * * *)</option>
                      <option value="daily">Daily at 3 AM (0 3 * * *)</option>
                      <option value="weekly">Weekly (0 0 * * 0)</option>
                    </select>
                  </div>

                  <div className="form-group-row">
                    <div className="toggle-switch-wrapper">
                      <input
                        type="checkbox"
                        id="breaking-enabled"
                        checked={breakingEnabled}
                        onChange={(e) => setBreakingEnabled(e.target.checked)}
                        className="toggle-checkbox"
                      />
                      <label htmlFor="breaking-enabled" className="toggle-label"></label>
                    </div>
                    <div className="toggle-text">
                      <span className="toggle-title font-semibold">Enable Breaking News Interrupts</span>
                      <span className="toggle-desc text-muted">Generate immediate broadcasts if importance score exceeds 0.85.</span>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* Storage Tab */}
            {activeSettingsTab === 'storage' && (
              <div className="settings-section">
                <h3 className="heading-md mb-3">Local Storage & Directories</h3>
                <p className="text-muted mb-4">
                  Objective preserves absolute user control. All raw documents, graph nodes, and embeddings are stored inside this repository's local database directory.
                </p>

                <div className="settings-form-grid">
                  <div className="form-group">
                    <label>Database root directory</label>
                    <input
                      type="text"
                      value=".objective/db/"
                      disabled
                      className="form-input cursor-not-allowed"
                    />
                  </div>

                  <div className="form-group">
                    <label>Gzip Archive Partitioning</label>
                    <select disabled className="form-input cursor-not-allowed">
                      <option>Year/Month Partitioning (Gzip JSON)</option>
                    </select>
                  </div>

                  {liveConfig && (
                    <div className="live-config">
                      <h4>Live Configuration</h4>
                      <dl className="config-list">
                        <div><dt>REST port</dt><dd>{liveConfig.rest_port}</dd></div>
                        <div><dt>WebSocket port</dt><dd>{liveConfig.websocket_port}</dd></div>
                        <div><dt>Data root</dt><dd>{liveConfig.data_root}</dd></div>
                        <div><dt>Document path</dt><dd>{liveConfig.document_path}</dd></div>
                        <div><dt>Vector path</dt><dd>{liveConfig.vector_path}</dd></div>
                        <div><dt>NATS URL</dt><dd>{liveConfig.nats_url}</dd></div>
                        <div><dt>Log level</dt><dd>{liveConfig.log_level}</dd></div>
                        <div><dt>Auth enabled</dt><dd>{liveConfig.auth_enabled ? 'yes' : 'no'}</dd></div>
                      </dl>
                    </div>
                  )}
                  {configError && (
                    <p className="text-muted">Live config unavailable: {configError}</p>
                  )}

                  <div className="form-group">
                    <a
                      className="btn-secondary export-btn"
                      href={api.exportDataUrl()}
                      download
                    >
                      <Download size={14} />
                      Export dataset (JSON)
                    </a>
                  </div>
                </div>
              </div>
            )}

            <div className="settings-footer mt-4">
              <button type="submit" className="btn-primary">Apply Changes</button>
            </div>

          </form>
        </div>
      </div>

      <style>{`
        .settings-container {
          height: 100%;
        }

        .settings-layout {
          display: grid;
          grid-template-columns: 240px 1fr;
          gap: 1.5rem;
          align-items: start;
        }

        @media (max-width: 768px) {
          .settings-layout {
            grid-template-columns: 1fr;
          }
        }

        .settings-nav {
          display: flex;
          flex-direction: column;
          padding: 0.75rem !important;
          gap: 0.25rem;
        }

        .settings-nav-btn {
          display: flex;
          align-items: center;
          gap: 0.75rem;
          padding: 0.65rem 1rem;
          border-radius: var(--radius-sm);
          color: var(--text-secondary);
          font-size: 0.9rem;
          font-weight: 600;
          text-align: left;
          transition: background var(--transition-fast), color var(--transition-fast);
        }

        .settings-nav-btn:hover {
          background: rgba(255, 255, 255, 0.02);
          color: var(--text-primary);
        }

        .settings-nav-btn.active {
          background: rgba(0, 122, 255, 0.08);
          color: var(--accent-blue);
        }

        .settings-form-grid {
          display: flex;
          flex-direction: column;
          gap: 1.25rem;
          margin-top: 1rem;
        }

        .form-group {
          display: flex;
          flex-direction: column;
          gap: 0.5rem;
        }

        .form-group label {
          font-size: 0.8rem;
          font-weight: 700;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: 0.05em;
        }

        .cursor-not-allowed {
          cursor: not-allowed;
          opacity: 0.6;
        }

        .model-download-status {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 0.75rem 1rem !important;
          background: rgba(255, 255, 255, 0.01) !important;
        }

        .download-info {
          display: flex;
          align-items: center;
          gap: 0.75rem;
        }

        .text-purple {
          color: var(--accent-blue);
        }

        .download-title {
          display: block;
          font-size: 0.85rem;
          font-weight: 600;
          color: var(--text-primary);
        }

        .download-desc {
          display: block;
          font-size: 0.75rem;
          color: var(--text-secondary);
        }

        /* Toggle Switch */
        .form-group-row {
          display: flex;
          align-items: center;
          gap: 1rem;
          background: var(--bg-tertiary);
          padding: 1rem;
          border-radius: var(--radius-sm);
          border: 1px solid var(--border-color);
        }

        .toggle-switch-wrapper {
          position: relative;
          width: 44px;
          height: 22px;
          flex-shrink: 0;
        }

        .toggle-checkbox {
          display: none;
        }

        .toggle-label {
          display: block;
          width: 100%;
          height: 100%;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          border-radius: var(--radius-full);
          cursor: pointer;
          position: relative;
          transition: background var(--transition-fast);
        }

        .toggle-label::after {
          content: '';
          position: absolute;
          width: 16px;
          height: 16px;
          border-radius: 50%;
          background: var(--text-secondary);
          top: 2px;
          left: 2px;
          transition: transform var(--transition-fast), background var(--transition-fast);
        }

        .toggle-checkbox:checked + .toggle-label {
          background: var(--accent-blue);
        }

        .toggle-checkbox:checked + .toggle-label::after {
          transform: translateX(22px);
          background: var(--text-primary);
        }

        .toggle-text {
          display: flex;
          flex-direction: column;
          text-align: left;
        }

        .toggle-title {
          font-size: 0.9rem;
          color: var(--text-primary);
        }

        .toggle-desc {
          font-size: 0.75rem;
        }

        .settings-footer {
          border-top: 1px solid var(--border-color);
          padding-top: 1.25rem;
        }

        .live-config {
          padding: 1rem !important;
          display: flex;
          flex-direction: column;
          gap: 0.5rem;
        }

        .live-config h4 {
          margin: 0 0 0.5rem 0;
          font-size: 0.9rem;
          font-weight: 700;
          text-transform: uppercase;
          letter-spacing: 0.05em;
          color: var(--text-secondary);
        }

        .config-list {
          display: grid;
          grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
          gap: 0.5rem 1rem;
          margin: 0;
        }

        .config-list > div {
          display: flex;
          flex-direction: column;
          gap: 0.15rem;
        }

        .config-list dt {
          font-size: 0.7rem;
          color: var(--text-muted);
          text-transform: uppercase;
        }

        .config-list dd {
          margin: 0;
          font-size: 0.85rem;
          color: var(--text-primary);
          font-family: var(--font-mono, monospace);
          word-break: break-all;
        }

        .export-btn {
          display: inline-flex;
          align-items: center;
          gap: 0.5rem;
          width: fit-content;
        }
      `}</style>
    </div>
  );
};
