import { useState, type ReactNode } from 'react';
import { useTelemetry } from './api';
import { PhaseSpacePlot } from './components/PhaseSpacePlot';
import { SprtChart } from './components/SprtChart';
import { ProcessTable, Status } from './components/ProcessTable';
import { RollbackLedger, formatBytes } from './components/RollbackLedger';
import { SimulatorPanel } from './components/SimulatorPanel';
import { CanaryPanel } from './components/CanaryPanel';
import { ResearchPanel } from './components/ResearchPanel';

type View = 'dashboard' | 'processes' | 'recovery' | 'canaries' | 'simulator' | 'research';
const navigation: { id: View; icon: string; label: string }[] = [
  { id: 'dashboard', icon: '◫', label: 'Live telemetry' },
  { id: 'processes', icon: '⌁', label: 'Process lineage' },
  { id: 'recovery', icon: '▤', label: 'Recovery center' },
  { id: 'canaries', icon: '◇', label: 'Canary matrix' },
  { id: 'simulator', icon: '△', label: 'Workload simulator' },
];

export default function App() {
  const { telemetry, connected, error, refresh } = useTelemetry();
  const [view, setView] = useState<View>('dashboard');
  const latestRun = telemetry?.runs[0];
  const totalWrites = telemetry?.processes.reduce((sum, process) => sum + process.writes, 0) ?? 0;
  const active = telemetry?.processes.filter(process => ['ACTIVE','FROZEN'].includes(process.status)).length ?? 0;
  const title = view === 'research' ? 'Offline research' : navigation.find(item => item.id === view)?.label ?? 'Live telemetry';
  return <div className="layout">
    <aside className="sidebar">
      <div className="brand"><span className="brand-icon"><i/>◈</span><div><b>AegisFS</b><small>SECURITY RESEARCH</small></div></div>
      <div className="side-label">OPERATIONS</div>
      <nav>{navigation.map(item => <button key={item.id} className={`nav ${view === item.id ? 'active' : ''}`} onClick={() => setView(item.id)}><span className="nav-icon">{item.icon}</span><span>{item.label}</span>{item.id === 'dashboard' && connected && <i className="nav-live"/>}</button>)}</nav>
      <div className="side-label">EVALUATION</div>
      <button className={`nav ${view === 'research' ? 'active' : ''}`} onClick={() => setView('research')}><span className="nav-icon">⌗</span><span>Offline research</span></button>
      <div className="lab-card"><span>ISOLATED LAB</span><b>No host protection</b><p>All writes are synthetic and confined to daemon-owned directories.</p></div>
      <div className="side-bottom"><span className={`connection-dot ${connected ? 'online' : ''}`}/>{connected ? 'DAEMON CONNECTED' : 'DAEMON OFFLINE'}<small>Research build · v0.2.0</small></div>
    </aside>
    <main>
      <header className="topbar"><div className="crumb">AEGISFS <span>/</span> {title.toUpperCase()}</div><div className="head-right"><span className="mount-status"><i/> MOUNT: NOT MOUNTED</span><span className={`badge ${connected ? 'online' : ''}`}><span className={`connection-dot ${connected ? 'online' : ''}`}/>{connected ? 'LIVE LAB' : 'OFFLINE'}</span></div></header>
      <div className="content">
        <div className="title-row"><div><div className="eyebrow">INTELLIGENT CONTAINMENT / TRANSACTIONAL RECOVERY</div><h1>{title}</h1><p>{subtitle(view)}</p></div>{view !== 'simulator' && <button className="button primary" onClick={() => setView('simulator')}>△ Run safe workload</button>}</div>
        <div className="notice"><b>◈ Research prototype — not a protection system</b><span>FUSE/eBPF interception and host containment are not active. Lab rollback is verified by directory-tree hash.</span></div>
        {error && !connected && <div className="connection-warning">Live daemon unavailable ({error}). Start <code>aegis-daemon</code>; offline reports remain available.</div>}

        {view === 'dashboard' && <>
          <div className="stats"><Stat label="ACTIVE TREES" value={active.toString()} sub="Synthetic RAGIDs" icon="⌁"/><Stat label="WRITE EVENTS" value={totalWrites.toLocaleString()} sub="Observed lab bursts" icon="↗"/><Stat label="COW PRE-IMAGES" value={formatBytes(telemetry?.status.cow_bytes ?? 0)} sub="Last recovery ledger" icon="▤"/><Stat label="TIME TO CONTAIN" value={latestRun?.contained ? `${telemetry?.processes.find(p => p.ragid === latestRun.ragid)?.containment_ms ?? 0} ms` : '—'} sub={latestRun ? `${latestRun.touched_files} files touched` : 'No attack run'} icon="◷"/></div>
          <div className="dashboard-grid"><Card title="Randomness phase space" subtitle="Entropy vs. χ² uniformity · live write bursts" tag="DIFFERENTIAL FEATURES" className="phase-card"><PhaseSpacePlot observations={telemetry?.observations ?? []}/></Card><Card title="Behavioral decision trajectories" subtitle="Lineage-aggregated, time-decayed evidence" tag="SPRT / CUSUM" className="sprt-card"><SprtChart processes={telemetry?.processes ?? []} threshold={telemetry?.status.threshold ?? 9.21034}/></Card></div>
          <div className="dashboard-grid lower"><Card title="Process trees" subtitle="Suspicion is aggregated at the root ancestor" tag={`${telemetry?.processes.length ?? 0} GROUPS`}><ProcessTable processes={telemetry?.processes.slice(0,5) ?? []} onChanged={refresh}/></Card><Card title="Atomic recovery ledger" subtitle="Strict reverse chronological replay" tag="BLAKE3 VERIFIED"><RollbackLedger entries={telemetry?.ledger.slice(0,6) ?? []} latestRun={latestRun} onChanged={refresh}/></Card></div>
        </>}

        {view === 'processes' && <Card title="Root ancestor inspector" subtitle="PID + start-time aware lineage primitives; displayed workloads are simulated" tag="EXPLICIT LAB SCOPE"><ProcessTable processes={telemetry?.processes ?? []} onChanged={refresh} expanded/></Card>}

        {view === 'recovery' && <>
          <div className="stats compact"><Stat label="LAST RUN" value={latestRun?.status.replace(/_/g,' ') ?? '—'} sub={latestRun?.workload.replace(/_/g,' ')}/><Stat label="FILES TOUCHED" value={latestRun?.touched_files.toString() ?? '—'} sub="Before freeze or completion"/><Stat label="RESTORED" value={latestRun ? formatBytes(latestRun.restored_bytes) : '—'} sub={`${latestRun?.restored_operations ?? 0} inverse operations`}/><Stat label="DATA LOSS" value={latestRun?.recovered ? '0 B' : '—'} sub="When pre/post tree hashes match"/></div>
          <Card title="Transactional CoW journal" subtitle="Conflict-checked, content-addressed whole-file pre-images" tag="REVERSE REPLAY"><RollbackLedger entries={telemetry?.ledger ?? []} latestRun={latestRun} onChanged={refresh}/></Card>
        </>}

        {view === 'canaries' && <Card title="Deployment & trap matrix" subtitle="Structurally valid decoys with deterministic BLAKE3 identities" tag="3 CANARY TYPES"><CanaryPanel canaries={telemetry?.canaries ?? []} onChanged={refresh}/></Card>}

        {view === 'simulator' && <>
          <Card title="Adversarial workload laboratory" subtitle="New empty directories only · fixed public keys · mandatory cleanup rollback" tag="SAFE BY CONSTRUCTION"><SimulatorPanel activeRun={telemetry?.status.active_run} onChanged={refresh}/></Card>
          <Card title="Run history" subtitle="Containment and recovery results, not benchmark claims" tag={`${telemetry?.runs.length ?? 0} RUNS`}><div className="table-wrap"><table><thead><tr><th>WORKLOAD</th><th>FILES</th><th>CONTAINED</th><th>RECOVERED</th><th>TREE HASH</th><th>STATE</th></tr></thead><tbody>{telemetry?.runs.map(run => <tr key={run.id}><td><b>{run.workload.replace(/_/g,' ')}</b><small className="table-sub">{run.id}</small></td><td>{run.touched_files}</td><td>{run.contained ? 'Yes' : 'No'}</td><td>{run.recovered ? '100%' : '—'}</td><td>{run.recovered && run.pre_tree_blake3 === run.post_tree_blake3 ? <span className="verified">✓ MATCH</span> : <span className="dim">PENDING</span>}</td><td><Status status={run.status}/></td></tr>)}</tbody></table>{!telemetry?.runs.length && <div className="table-empty">No workload runs yet.</div>}</div></Card>
        </>}

        {view === 'research' && <Card title="Offline benchmark & replay" subtitle="Independent of the live synthetic telemetry stream" tag="LOCAL FILES"><ResearchPanel/></Card>}
        <footer>AEGISFS / ISOLATED RESEARCH BUILD <span>Mathematical evidence is uncalibrated · no production FPR, latency, or RPO guarantee</span></footer>
      </div>
    </main>
  </div>;
}

function Card({ title, subtitle, tag, className = '', children }: { title: string; subtitle: string; tag: string; className?: string; children: ReactNode }) {
  return <section className={`card ${className}`}><div className="card-head"><div><h2>{title}</h2><p>{subtitle}</p></div><span className="card-tag">{tag}</span></div>{children}</section>;
}
function Stat({ label, value, sub, icon }: { label: string; value: string; sub?: string; icon?: string }) {
  return <div className="stat"><div>{label}{icon && <span>{icon}</span>}</div><strong title={value}>{value}</strong>{sub && <small>{sub}</small>}</div>;
}
function subtitle(view: View) {
  switch (view) {
    case 'dashboard': return 'Live differential randomness, sequential evidence, and verified transactional recovery.';
    case 'processes': return 'Inspect synthetic process trees and their aggregated decision state.';
    case 'recovery': return 'Audit every pre-image and inverse operation in the isolated CoW journal.';
    case 'canaries': return 'Deploy and verify high-priority structured document traps.';
    case 'simulator': return 'Evaluate benign compression and adversarial transformations without touching user data.';
    case 'research': return 'Explore reproducible offline classifier and scoring reports.';
  }
}
