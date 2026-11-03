import { useState } from 'react';

type ReplayEvent={group:string;elapsed_seconds:number;score:number;decision:string};
type Replay={schema:string;warning:string;threshold:number;events:ReplayEvent[]};
const colors=['#68d7bc','#f37e86','#d8b173','#a1a2f5','#84b9e6'];

export function ReplayPanel(){
  const [report,setReport]=useState<Replay|null>(null);
  const [error,setError]=useState('');
  async function load(file?:File){
    if(!file)return;
    try {
      const r=JSON.parse(await file.text()) as Replay;
      if(r.schema!=='aegisfs-replay-v1'||!Number.isFinite(r.threshold)||r.threshold<=0||!Array.isArray(r.events)||r.events.length===0||r.events.length>10000||!r.events.every(e=>typeof e.group==='string'&&Number.isFinite(e.elapsed_seconds)&&Number.isFinite(e.score))) throw new Error('Invalid or unsupported replay report');
      setReport(r);setError('');
    } catch(e){setReport(null);setError(e instanceof Error?e.message:'Invalid report');}
  }
  const grouped=new Map<string,ReplayEvent[]>();
  report?.events.forEach(e=>{const list=grouped.get(e.group)??[];list.push(e);grouped.set(e.group,list)});
  const groups=Array.from(grouped.entries());
  const maxTime=Math.max(0.001,...(report?.events.map(e=>e.elapsed_seconds)??[]));
  const maxScore=Math.max(0.001,(report?.threshold??1)*1.2,...(report?.events.map(e=>e.score)??[]));
  const x=(t:number)=>45+540*t/maxTime, y=(score:number)=>190-180*score/maxScore;
  return <section className="card replay-card"><div className="card-head"><div><h2>Offline score replay</h2><p>Illustrative per-group trajectories · no process monitoring</p></div><label className="replay-upload">↑ &nbsp; Load replay JSON<input type="file" accept=".json,application/json" onChange={e=>load(e.target.files?.[0])}/></label></div>
    {error&&<div className="error replay-error">{error}</div>}
    {!report?<div className="replay-empty">Load an <code>aegis-replay</code> report to inspect synthetic trajectories.</div>:<><div className="replay-warning">UNCALIBRATED · Threshold crossings are not containment events or probability guarantees.</div><div className="scatter"><svg viewBox="0 0 600 210" preserveAspectRatio="none" aria-label="Offline per-group score trajectories">
      <g stroke="#24323f" strokeWidth="1">{[10,55,100,145,190].map(v=><line key={v} x1="45" x2="585" y1={v} y2={v}/>)}</g>
      <line x1="45" x2="585" y1={y(report.threshold)} y2={y(report.threshold)} stroke="#ef8186" strokeDasharray="5 5"/>
      <g fill="#8297a1" fontSize="10"><text x="4" y="14">{maxScore.toFixed(1)}</text><text x="10" y="192">0</text><text x="45" y="205">0s</text><text x="555" y="205">{maxTime.toFixed(1)}s</text></g>
      {groups.map(([name,events],i)=><polyline key={name} fill="none" stroke={colors[i%colors.length]} strokeWidth="2" points={events.map(e=>`${x(e.elapsed_seconds)},${y(e.score)}`).join(' ')}/>)}</svg><div>ELAPSED TIME (SECONDS)</div></div><div className="legend">{groups.map(([name,events],i)=><span key={name}><i style={{background:colors[i%colors.length]}}/>{name} · {events.some(e=>e.decision==='above_threshold')?'crossed':'not crossed'}</span>)}<span className="legend-note">Dashed line: illustrative threshold</span></div></>}
  </section>
}
