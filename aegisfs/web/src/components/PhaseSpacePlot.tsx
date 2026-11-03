import type { Observation } from '../types';

export function PhaseSpacePlot({ observations }: { observations: Observation[] }) {
  const points = observations.filter(point => point.features.chi2_p !== null).slice(-700);
  const x = (entropy: number) => 48 + Math.max(0, Math.min(8, entropy)) / 8 * 652;
  const y = (p: number) => {
    const decades = Math.max(0, Math.min(6, -Math.log10(Math.max(1e-6, p))));
    return 18 + decades / 6 * 218;
  };
  return <div className="phase-plot">
    <svg viewBox="0 0 740 278" role="img" aria-label="Entropy and chi-square p-value phase space">
      <defs>
        <linearGradient id="crypto-zone" x1="0" y1="0" x2="1" y2="1"><stop stopColor="#f26c78" stopOpacity=".12"/><stop offset="1" stopColor="#f26c78" stopOpacity=".015"/></linearGradient>
        <linearGradient id="compression-zone"><stop stopColor="#d6aa5c" stopOpacity=".025"/><stop offset="1" stopColor="#d6aa5c" stopOpacity=".10"/></linearGradient>
      </defs>
      <rect x={x(7.7)} y={y(.99)} width={700-x(7.7)} height={y(.01)-y(.99)} rx="7" fill="url(#crypto-zone)" stroke="#7e3d4a" strokeDasharray="4 5"/>
      <rect x={x(7.5)} y={y(.001)} width={700-x(7.5)} height={236-y(.001)} rx="7" fill="url(#compression-zone)"/>
      {[0,1,2,3,4,5,6].map(decade => <g key={decade}><line x1="48" x2="700" y1={18+decade/6*218} y2={18+decade/6*218} stroke="#202d38"/><text x="39" y={22+decade/6*218} textAnchor="end" className="axis-text">{decade === 0 ? '1' : `10⁻${decade}`}</text></g>)}
      {[0,2,4,6,8].map(value => <g key={value}><line x1={x(value)} x2={x(value)} y1="18" y2="236" stroke="#202d38"/><text x={x(value)} y="254" textAnchor="middle" className="axis-text">{value.toFixed(1)}</text></g>)}
      <text x={x(7.77)} y={y(.8)+4} className="zone-label crypto-label">CRYPTO-UNIFORM REGION</text>
      <text x={x(7.57)} y={228} className="zone-label compression-label">STRUCTURAL REJECTION REGION</text>
      {points.map((point, index) => <circle key={point.sequence} cx={x(point.features.entropy_post)} cy={y(point.features.chi2_p!)} r={point.canary ? 5 : 3.4} className={point.crypto_uniform ? 'point crypto' : 'point benign'} opacity={.4 + .6*(index+1)/points.length}><title>{point.path} · H={point.features.entropy_post.toFixed(3)} · p={point.features.chi2_p}</title></circle>)}
      <text x="374" y="273" textAnchor="middle" className="axis-title">SHANNON ENTROPY (BITS / BYTE)</text>
      <text x="12" y="127" textAnchor="middle" className="axis-title" transform="rotate(-90 12 127)">χ² UPPER-TAIL P-VALUE</text>
    </svg>
    {points.length === 0 && <div className="plot-empty"><span>◌</span><b>Awaiting write observations</b><small>Run a safe workload from the simulator.</small></div>}
    <div className="legend"><span><i className="dot benign-dot"/> Structural / benign</span><span><i className="dot crypto-dot"/> Crypto-uniform evidence</span><span className="legend-note">A p-value is not an encryption probability.</span></div>
  </div>;
}
