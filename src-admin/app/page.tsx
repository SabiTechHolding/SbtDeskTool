import type { Metadata } from "next";
import AdminControls from "./AdminControls";
import { loadDashboardData } from "./dashboard-data";
import { getAdminAccess } from "./admin-auth";

export const dynamic = "force-dynamic";
export const metadata:Metadata = { title:"SbtDesk Translation Admin",description:"Enterprise Dictionary and Translation Memory operations console." };

export default async function Home() {
  const workspaceId=process.env.DEFAULT_WORKSPACE_ID??"local";
  const access=await getAdminAccess(workspaceId);
  if(!access.allowed||!access.role) return <main className="denied"><section><span>403</span><h1>Cloudflare Access required</h1><p>{access.user.email==="not-authenticated"?"Protect this Worker with a Cloudflare Access policy before opening the dashboard.":`${access.user.email} is authenticated but has no membership in this workspace.`}</p></section></main>;
  const data=await loadDashboardData(workspaceId);
  const initials=access.user.displayName.split(/\s+/).map((part)=>part[0]).join("").slice(0,2).toUpperCase();
  return <div className="shell">
    <aside>
      <div className="brand"><span className="brandMark">S</span><div><strong>SbtDesk</strong><small>Translation Admin</small></div></div>
      <nav aria-label="Primary navigation"><a className="active" href="#overview"><span>01</span>Overview</a><a href="#review"><span>02</span>Review queue <b>{data.reviewCount}</b></a><a href="#sync"><span>03</span>Sync devices</a><a href="#members"><span>04</span>Members & roles</a><a href="#audit"><span>05</span>Audit log</a></nav>
      <div className="workspace"><small>WORKSPACE</small><button><span className="avatar">SB</span><span><strong>{workspaceId}</strong><small>Enterprise</small></span></button></div>
    </aside>
    <main>
      <header className="topbar"><div><p>Operations / Overview</p><h1>Translation operations</h1></div><div className="topActions"><span className="apiState"><i className={data.connected?"connected":""}/>{data.connected?"D1 connected":"D1 unavailable"}</span><span className="roleBadge">{access.role}</span><span className="user" title={access.user.email}>{initials}</span></div></header>
      {!data.connected&&<section className="notice"><span>!</span><div><strong>Apply the D1 migrations to enable live data</strong><p>The admin is authenticated, but its database binding or schema is unavailable.</p></div></section>}
      <section id="overview" className="metrics" aria-label="Translation metrics"><article><div><span>Dictionary terms</span></div><b>{data.dictionaryCount.toLocaleString()}</b><em className="up">Approved shared terms</em></article><article><div><span>Memory segments</span></div><b>{data.memoryCount.toLocaleString()}</b><em className="up">Reusable translations</em></article><article><div><span>Awaiting review</span></div><b>{data.reviewCount.toLocaleString()}</b><em>Unresolved memory entries</em></article><article><div><span>Registered devices</span></div><b>{data.deviceCount.toLocaleString()}</b><em className="warn">Workspace sync clients</em></article></section>
      <AdminControls workspaceId={workspaceId} role={access.role} reviewItems={data.reviewItems} members={data.members} deviceTokens={data.deviceTokens}/>
      <section className="lowerGrid">
        <article id="sync" className="panel compact"><div className="panelHead"><div><h2>Sync health</h2><p>Desktop cursor activity by device.</p></div><span className="healthy">{data.connected?"Healthy":"Unavailable"}</span></div>{data.devices.map((device)=><div className="syncRow" key={device.id}><span className="device">W</span><div><strong>{device.id}</strong><small>Windows desktop</small></div><span><strong>Cursor {device.cursor}</strong><small>{new Date(device.lastSeen).toLocaleString()}</small></span></div>)}{!data.devices.length&&<p className="emptyState">No desktop devices have synced yet.</p>}</article>
        <article id="audit" className="panel compact"><div className="panelHead"><div><h2>Recent activity</h2><p>Review, membership and sync audit trail.</p></div></div><ol className="activity">{data.activities.map((entry,index)=><li key={`${entry.createdAt}-${entry.actor}-${index}`}><span className="dot blue"/><div><strong>{entry.action.replaceAll("_"," ")}</strong><p>{entry.detail}</p><small>{entry.actor} · {new Date(entry.createdAt).toLocaleString()}</small></div></li>)}{!data.activities.length&&<li className="emptyState">No audit activity yet.</li>}</ol></article>
      </section>
    </main>
  </div>;
}
