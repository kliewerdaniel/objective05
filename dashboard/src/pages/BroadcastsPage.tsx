import React, { useState } from 'react';
import { useFeedStore } from '../store/feedStore';
import { useUiStore } from '../store/uiStore';
import { api } from '../api/client';
import { Radio, Play, Download, ChevronRight, Square, Zap } from 'lucide-react';
import type { Broadcast } from '../api/types';

export const BroadcastsPage: React.FC = () => {
  const { broadcasts } = useFeedStore();
  const { addNotification } = useUiStore();
  const [selectedBroadcast, setSelectedBroadcast] = useState<Broadcast | null>(null);
  const [generating, setGenerating] = useState(false);
  
  // Custom audio player state
  const [isPlaying, setIsPlaying] = useState(false);
  const [playbackSpeed, setPlaybackSpeed] = useState(1);
  const [currentTime, setCurrentTime] = useState(0);

  const getDurationString = (secs?: number) => {
    if (!secs) return '0:00';
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${s < 10 ? '0' : ''}${s}`;
  };

  const handlePlayToggle = () => {
    setIsPlaying(!isPlaying);
  };

  const handleGenerate = async () => {
    setGenerating(true);
    try {
      const response = await api.generateBroadcast({});
      addNotification(`Broadcast "${response.broadcast.title}" generated.`, 'success');
      setSelectedBroadcast(response.broadcast);
    } catch (err: any) {
      addNotification(`Failed to generate: ${err.message}`, 'error');
    } finally {
      setGenerating(false);
    }
  };

  return (
    <div className="broadcasts-container animate-fade-in">
      <div className="broadcasts-layout">
        {/* Left Side: Broadcast Feed */}
        <div className="broadcasts-list">
          <div className="broadcasts-list-header">
            <h2 className="heading-md">Broadcast Archives</h2>
            <button
              className="btn-primary generate-btn"
              onClick={handleGenerate}
              disabled={generating}
            >
              <Zap size={14} />
              {generating ? 'Generating…' : 'Generate Now'}
            </button>
          </div>
          {broadcasts.length === 0 ? (
            <div className="empty-state glass-card">
              <Radio size={48} className="text-muted" />
              <p className="heading-md">No broadcasts generated yet</p>
              <p className="text-muted">Broadcast scheduler will trigger updates hourly. Use CLI to trigger manual briefing.</p>
            </div>
          ) : (
            <div className="briefings-grid">
              {broadcasts.map((b: Broadcast) => (
                <div
                  key={b.id}
                  onClick={() => {
                    setSelectedBroadcast(b);
                    setIsPlaying(false);
                    setCurrentTime(0);
                  }}
                  className={`briefing-card glass-card ${selectedBroadcast?.id === b.id ? 'active' : ''}`}
                >
                  <div className="b-header">
                    <span className="badge badge-purple">AUDIO BRIEFING</span>
                    <span className="b-date">
                      {new Date(b.created_at).toLocaleDateString(undefined, {
                        month: 'short',
                        day: 'numeric',
                        hour: '2-digit',
                        minute: '2-digit',
                      })}
                    </span>
                  </div>

                  <h3 className="b-title">{b.title}</h3>
                  <p className="b-summary">{b.summary}</p>
                  
                  <div className="b-footer">
                    <span className="b-duration">Duration: {getDurationString(b.duration_seconds)}</span>
                    <ChevronRight size={16} className="b-chevron" />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Right Side: Active Broadcast Transcript & Player */}
        <div className="broadcast-viewer">
          {selectedBroadcast ? (
            <div className="viewer-panel glass-panel">
              <div className="viewer-header">
                <h2 className="heading-lg">{selectedBroadcast.title}</h2>
                <div className="viewer-actions">
                  <button className="btn-secondary ctrl-btn" title="Download Audio">
                    <Download size={16} />
                  </button>
                </div>
              </div>

              {/* Custom Audio Player Widget */}
              <div className="audio-player-widget glass-card">
                <div className="player-controls">
                  <button onClick={handlePlayToggle} className="play-btn">
                    {isPlaying ? <Square size={20} fill="currentColor" /> : <Play size={20} fill="currentColor" className="play-icon-adjust" />}
                  </button>
                  
                  <div className="player-track-info">
                    <div className="track-time">
                      <span>{getDurationString(currentTime)}</span>
                      <span>{getDurationString(selectedBroadcast.duration_seconds)}</span>
                    </div>
                    <div className="progress-bar-container">
                      <div
                        className="progress-bar-fill"
                        style={{
                          width: `${(currentTime / (selectedBroadcast.duration_seconds || 1)) * 100}%`,
                        }}
                      ></div>
                    </div>
                  </div>

                  {/* Playback speed toggle */}
                  <button
                    onClick={() => setPlaybackSpeed(playbackSpeed === 1 ? 1.5 : playbackSpeed === 1.5 ? 2 : 1)}
                    className="speed-btn"
                  >
                    {playbackSpeed}x
                  </button>
                </div>
              </div>

              {/* Transcript Markdown Reading Area */}
              <div className="transcript-content">
                <div className="markdown-body">
                  {selectedBroadcast.content_markdown?.split('\n').map((line: string, idx: number) => {
                    if (line.startsWith('# ')) {
                      return <h1 key={idx} className="title-display mb-4">{line.replace('# ', '')}</h1>;
                    }
                    if (line.startsWith('### ')) {
                      return <h3 key={idx} className="heading-md mt-4 mb-2">{line.replace('### ', '')}</h3>;
                    }
                    if (line.startsWith('* ')) {
                      return <li key={idx} className="list-item ml-4">{line.replace('* ', '')}</li>;
                    }
                    if (line.trim().length === 0) return null;
                    return <p key={idx} className="paragraph mb-3">{line}</p>;
                  })}
                </div>
              </div>
            </div>
          ) : (
            <div className="viewer-placeholder glass-panel">
              <Radio size={48} className="text-muted animate-pulse-slow" />
              <h3 className="heading-md">Select a Broadcast</h3>
              <p className="text-muted">Choose a synthesized intelligence briefing from the archive to stream the audio and read the transcript.</p>
            </div>
          )}
        </div>
      </div>

      <style>{`
        .broadcasts-list-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-bottom: 1rem;
          gap: 1rem;
        }

        .generate-btn {
          font-size: 0.8rem;
          padding: 0.4rem 0.85rem;
        }

        .broadcasts-container {
          height: 100%;
        }

        .broadcasts-layout {
          display: grid;
          grid-template-columns: 380px 1fr;
          gap: 1.5rem;
          height: 100%;
          align-items: start;
        }

        @media (max-width: 1024px) {
          .broadcasts-layout {
            grid-template-columns: 1fr;
          }
        }

        .mb-3 { margin-bottom: 0.75rem; }
        .mb-4 { margin-bottom: 1rem; }
        .mt-4 { margin-top: 1.25rem; }
        .mb-2 { margin-bottom: 0.5rem; }
        .ml-4 { margin-left: 1rem; }

        .briefings-grid {
          display: flex;
          flex-direction: column;
          gap: 1rem;
        }

        .briefing-card {
          text-align: left;
          cursor: pointer;
          border: 1px solid var(--border-color);
          transition: border-color var(--transition-fast), background var(--transition-fast);
        }

        .briefing-card:hover {
          border-color: var(--border-color-hover);
          background: rgba(255, 255, 255, 0.01);
        }

        .briefing-card.active {
          border-color: var(--accent-primary);
          background: rgba(139, 92, 246, 0.04);
        }

        .b-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          font-size: 0.75rem;
        }

        .b-date {
          color: var(--text-muted);
        }

        .b-title {
          font-family: var(--font-display);
          font-size: 1.1rem;
          font-weight: 600;
          color: var(--text-primary);
          margin-top: 0.5rem;
        }

        .b-summary {
          font-size: 0.85rem;
          color: var(--text-secondary);
          line-height: 1.5;
          margin-top: 0.5rem;
        }

        .b-footer {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-top: 0.75rem;
          font-size: 0.75rem;
          color: var(--text-muted);
        }

        .b-chevron {
          color: var(--text-muted);
          transition: transform var(--transition-fast);
        }

        .briefing-card:hover .b-chevron {
          transform: translateX(3px);
          color: var(--text-primary);
        }

        /* Viewer */
        .viewer-panel {
          display: flex;
          flex-direction: column;
          gap: 1.5rem;
          text-align: left;
        }

        .viewer-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .audio-player-widget {
          padding: 1rem !important;
          background: rgba(255, 255, 255, 0.02) !important;
          border-radius: var(--radius-md);
        }

        .player-controls {
          display: flex;
          align-items: center;
          gap: 1rem;
        }

        .play-btn {
          width: 44px;
          height: 44px;
          border-radius: 50%;
          background: var(--accent-gradient);
          color: var(--text-primary);
          display: flex;
          align-items: center;
          justify-content: center;
          box-shadow: 0 4px 10px rgba(139, 92, 246, 0.3);
          transition: transform var(--transition-fast);
        }

        .play-btn:hover {
          transform: scale(1.05);
        }

        .play-icon-adjust {
          margin-left: 2px;
        }

        .player-track-info {
          flex-grow: 1;
          display: flex;
          flex-direction: column;
          gap: 0.35rem;
        }

        .track-time {
          display: flex;
          justify-content: space-between;
          font-size: 0.75rem;
          color: var(--text-muted);
          font-family: var(--font-mono);
        }

        .progress-bar-container {
          height: 5px;
          background: rgba(255, 255, 255, 0.05);
          border-radius: var(--radius-full);
          overflow: hidden;
          cursor: pointer;
        }

        .progress-bar-fill {
          height: 100%;
          background: var(--accent-gradient);
          border-radius: var(--radius-full);
        }

        .speed-btn {
          font-size: 0.8rem;
          font-weight: 700;
          color: var(--text-secondary);
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          padding: 0.25rem 0.5rem;
          border-radius: var(--radius-sm);
        }

        .transcript-content {
          max-height: 450px;
          overflow-y: auto;
          padding-right: 0.5rem;
        }

        .markdown-body {
          color: var(--text-secondary);
          line-height: 1.7;
          font-size: 0.95rem;
        }

        .paragraph {
          margin-bottom: 1rem;
        }

        .list-item {
          margin-bottom: 0.5rem;
        }

        .viewer-placeholder {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          padding: 6rem 2rem !important;
          gap: 1rem;
          text-align: center;
          min-height: 400px;
        }
      `}</style>
    </div>
  );
};
