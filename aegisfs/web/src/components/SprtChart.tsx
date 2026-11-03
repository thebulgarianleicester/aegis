import type { ProcessView } from '../types';

const colors = ['#67dfc0', '#f17680', '#8ea9ff', '#e0b96c', '#c195ef', '#64b5d9'];

export function SprtChart({ processes, threshold }: { processes: ProcessView[]; threshold: number }) {
  const shown = processes.filter(process => process.trajectory.length > 0).slice(0, 6);
  const maxTime = Math.max(20, ...shown.flatMap(process => process.trajectory.map(point => point.elapsed_ms)));
  const maxScore = Math.max(10, threshold * 1.08, ...shown.map(process => process.peak_score));
  const x = (time: number) => 48 + time / maxTime * 652;
  const y = (score: number) => 220 - Math.max(0, score) / maxScore * 200;
  return <div className="sprt-chart">
    <svg viewBox="0 0 740 266" role="img" aria-label="Time-decayed behavioral score trajectories">
      {[0, .25, .5, .75, 1].map(fraction => <g key={fraction}><line x1="48" x2="700" y1={y(maxScore*fraction)} y2={y(maxScore*fraction)} stroke="#202d38"/><text x="38" y={y(maxScore*fraction)+4} textAnchor="end" className="axis-text">{(maxScore*fraction).toFixed(1)}</text></g>)}
      <line x1="48" x2="700" y1={y(threshold)} y2={y(threshold)} stroke="#f17680" strokeDasharray="7 5" strokeWidth="1.5"/>
      <rect x="540" y={y(threshold)-20} width="160" height="17" rx="4" fill="#281c26"/><text x="693" y={y(threshold)-8} textAnchor="end" className="threshold-label">B = {threshold.toFixed(2)} · FREEZE</text>
      {shown.map((process, index) => {
        const path = process.trajectory.map((point, i) => `${i === 0 ? 'M' : 'L'} ${x(point.elapsed_ms).toFixed(2)} ${y(point.score).toFixed(2)}`).join(' ');
        return <g key={process.ragid}><path d={path} fill="none" stroke={colors[index]} strokeWidth="2.2" strokeLinejoin="round"/>{process.trajectory.length > 0 && <circle cx={x(process.trajectory[process.trajectory.length-1].elapsed_ms)} cy={y(process.score)} r="4" fill={colors[index]}/>}</g>;
      })}
      {[0, .5, 1].map(fraction => <text key={fraction} x={x(maxTime*fraction)} y="242" textAnchor="middle" className="axis-text">{Math.round(maxTime*fraction)} ms</text>)}
      <text x="374" y="262" textAnchor="middle" className="axis-title">ELAPSED LAB TIME</text>
    </svg>
    {shown.length === 0 && <div className="plot-empty"><span>⌁</span><b>No active trajectories</b><small>Process-tree evidence will appear here.</small></div>}
    <div className="legend trajectory-legend">{shown.map((process,index) => <span key={process.ragid}><i className="dot" style={{background:colors[index]}}/>{process.executable}<small> · {process.ragid.slice(0,12)}</small></span>)}</div>
  </div>;
}
