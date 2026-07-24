"use client";

import { useState } from "react";
import type { DashboardDeviceToken, DashboardItem, DashboardMember } from "./dashboard-data";
import type { WorkspaceRole } from "./admin-auth";

type Props = { workspaceId:string; role:WorkspaceRole; reviewItems:DashboardItem[]; members:DashboardMember[]; deviceTokens:DashboardDeviceToken[] };

export default function AdminControls({workspaceId,role,reviewItems,members,deviceTokens}:Props) {
  const [items,setItems] = useState(reviewItems);
  const [memberRows,setMemberRows] = useState(members);
  const [tokenRows,setTokenRows] = useState(deviceTokens);
  const [email,setEmail] = useState("");
  const [newRole,setNewRole] = useState<WorkspaceRole>("viewer");
  const [deviceId,setDeviceId] = useState("");
  const [deviceLabel,setDeviceLabel] = useState("");
  const [deviceRole,setDeviceRole] = useState<"reader"|"editor">("editor");
  const [expiryDays,setExpiryDays] = useState("90");
  const [issuedToken,setIssuedToken] = useState("");
  const [busy,setBusy] = useState("");
  const [message,setMessage] = useState("");

  async function send(payload:Record<string,unknown>) {
    setMessage("");
    const response = await fetch("/api/admin",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({workspaceId,...payload})});
    const result = await response.json() as {ok?:boolean;error?:string;token?:string;tokenId?:string};
    if (!response.ok || !result.ok) throw new Error(result.error ?? "Operation failed");
    return result;
  }
  async function review(item:DashboardItem,decision:"approved"|"rejected"|"conflict") {
    const key=`review:${item.id}`; setBusy(key);
    try {
      await send({action:"review",entityType:item.entityType,entityId:item.id,decision});
      setItems((current)=>current.map((row)=>row.id===item.id?{...row,status:decision[0].toUpperCase()+decision.slice(1)}:row));
      setMessage(`Saved ${decision} decision.`);
    } catch(error) { setMessage(error instanceof Error?error.message:"Operation failed"); }
    finally { setBusy(""); }
  }
  async function saveMember(event:React.FormEvent) {
    event.preventDefault(); const normalized=email.trim().toLowerCase(); if(!normalized)return; setBusy("member:add");
    try {
      await send({action:"upsert_member",email:normalized,role:newRole});
      setMemberRows((current)=>[...current.filter((row)=>row.email!==normalized),{email:normalized,role:newRole,createdAt:new Date().toISOString()}]);
      setEmail(""); setMessage("Member access saved.");
    } catch(error) { setMessage(error instanceof Error?error.message:"Operation failed"); }
    finally { setBusy(""); }
  }
  async function removeMember(memberEmail:string) {
    setBusy(`member:${memberEmail}`);
    try { await send({action:"remove_member",email:memberEmail}); setMemberRows((current)=>current.filter((row)=>row.email!==memberEmail)); setMessage("Member removed."); }
    catch(error) { setMessage(error instanceof Error?error.message:"Operation failed"); }
    finally { setBusy(""); }
  }
  async function createDeviceToken(event:React.FormEvent) {
    event.preventDefault(); setBusy("token:create"); setIssuedToken("");
    try {
      const days=Number(expiryDays);
      const expiresAt=Number.isFinite(days)&&days>0?new Date(Date.now()+days*86_400_000).toISOString():null;
      const result=await send({action:"create_device_token",deviceId:deviceId.trim(),label:deviceLabel.trim(),role:deviceRole,expiresAt});
      if(!result.token||!result.tokenId) throw new Error("Server did not return the one-time token");
      setIssuedToken(result.token);
      setTokenRows((current)=>[{id:result.tokenId!,deviceId:deviceId.trim(),label:deviceLabel.trim(),role:deviceRole,createdAt:new Date().toISOString(),expiresAt,revokedAt:null,lastUsedAt:null,active:true},...current]);
      setMessage("Device credential created. Copy it now; it will not be shown again.");
      setDeviceLabel("");
    } catch(error) { setMessage(error instanceof Error?error.message:"Operation failed"); }
    finally { setBusy(""); }
  }
  async function revokeDeviceToken(tokenId:string) {
    setBusy(`token:${tokenId}`);
    try {
      await send({action:"revoke_device_token",tokenId});
      const revokedAt=new Date().toISOString();
      setTokenRows((current)=>current.map((token)=>token.id===tokenId?{...token,revokedAt,active:false}:token));
      setMessage("Device credential revoked.");
    } catch(error) { setMessage(error instanceof Error?error.message:"Operation failed"); }
    finally { setBusy(""); }
  }

  return <>
    {message&&<p className="actionMessage" role="status">{message}</p>}
    <section id="review" className="panel">
      <div className="panelHead"><div><h2>Review queue</h2><p>Approve shared terminology and resolve competing translations.</p></div><span className="roleBadge">{role}</span></div>
      <div className="tableWrap"><table><thead><tr><th>Source</th><th>Translation</th><th>Direction</th><th>Provenance</th><th>Status</th><th>Actions</th></tr></thead><tbody>
        {items.map((item)=><tr key={`${item.entityType}:${item.id}`}><td>{item.source}</td><td>{item.translation}</td><td><span className="direction">{item.direction}</span></td><td>{item.sourceType}</td><td><span className={`status ${item.status.toLowerCase()}`}>{item.status}</span></td><td><div className="rowActions"><button disabled={busy===`review:${item.id}`} onClick={()=>review(item,"approved")}>Approve</button><button disabled={busy===`review:${item.id}`} onClick={()=>review(item,"conflict")}>Conflict</button><button disabled={busy===`review:${item.id}`} onClick={()=>review(item,"rejected")}>Reject</button></div></td></tr>)}
        {!items.length&&<tr><td colSpan={6} className="emptyState">No translation records to review.</td></tr>}
      </tbody></table></div>
    </section>
    <section id="members" className="panel">
      <div className="panelHead"><div><h2>Members & roles</h2><p>Viewer can read, reviewer can decide, admin can manage access.</p></div></div>
      {role==="admin"&&<form className="memberForm" onSubmit={saveMember}><input type="email" required value={email} onChange={(event)=>setEmail(event.target.value)} placeholder="user@company.com" aria-label="Member email"/><select value={newRole} onChange={(event)=>setNewRole(event.target.value as WorkspaceRole)} aria-label="Member role"><option value="viewer">Viewer</option><option value="reviewer">Reviewer</option><option value="admin">Admin</option></select><button className="primary" disabled={busy==="member:add"}>Save member</button></form>}
      <div className="memberList">{memberRows.map((member)=><div className="memberRow" key={member.email}><div><strong>{member.email}</strong><small>Added {new Date(member.createdAt).toLocaleDateString()}</small></div><span className="roleBadge">{member.role}</span>{role==="admin"&&<button className="danger" disabled={busy===`member:${member.email}`} onClick={()=>removeMember(member.email)}>Remove</button>}</div>)}{!memberRows.length&&<p className="emptyState">No workspace members yet. The bootstrap allowlist still has admin access.</p>}</div>
    </section>
    <section id="device-credentials" className="panel">
      <div className="panelHead"><div><h2>Device credentials</h2><p>Issue a unique, revocable sync token for each desktop. Tokens are stored only as SHA-256 hashes.</p></div></div>
      {role==="admin"&&<form className="tokenForm" onSubmit={createDeviceToken}><input required pattern="[A-Za-z0-9_.:-]{1,120}" value={deviceId} onChange={(event)=>setDeviceId(event.target.value)} placeholder="device-id" aria-label="Device ID"/><input value={deviceLabel} maxLength={120} onChange={(event)=>setDeviceLabel(event.target.value)} placeholder="Label (optional)" aria-label="Device label"/><select value={deviceRole} onChange={(event)=>setDeviceRole(event.target.value as "reader"|"editor")} aria-label="Sync role"><option value="editor">Editor</option><option value="reader">Reader</option></select><input type="number" min="1" max="3650" value={expiryDays} onChange={(event)=>setExpiryDays(event.target.value)} aria-label="Expiry days"/><button className="primary" disabled={busy==="token:create"}>Create token</button></form>}
      {issuedToken&&<div className="issuedToken"><strong>One-time token</strong><code>{issuedToken}</code><button type="button" onClick={async()=>{await navigator.clipboard.writeText(issuedToken);setMessage("Token copied.");}}>Copy</button><button type="button" onClick={()=>setIssuedToken("")}>Hide</button></div>}
      <div className="memberList">{tokenRows.map((token)=><div className="tokenRow" key={token.id}><div><strong>{token.label||token.deviceId}</strong><small>{token.deviceId} · Created {new Date(token.createdAt).toLocaleDateString()}{token.lastUsedAt?` · Last used ${new Date(token.lastUsedAt).toLocaleString()}`:" · Never used"}</small></div><span className="roleBadge">{token.role}</span><span className={`status ${token.active?"approved":"rejected"}`}>{token.revokedAt?"Revoked":token.active?"Active":"Expired"}</span>{role==="admin"&&token.active&&<button className="danger" disabled={busy===`token:${token.id}`} onClick={()=>revokeDeviceToken(token.id)}>Revoke</button>}</div>)}{!tokenRows.length&&<p className="emptyState">No per-device credentials have been issued.</p>}</div>
    </section>
  </>;
}
