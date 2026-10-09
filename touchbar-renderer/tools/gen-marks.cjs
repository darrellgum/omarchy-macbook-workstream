// Bakes Grok Bot mark geometry (body + eye paths per pose) from the extracted app module.
const fs=require('fs');const path=require('path');
const src=fs.readFileSync(process.argv[2],'utf8');
const m1t=["blob","pebble","bean","egg","squircle","tablet","capsule","cylinder","hex","gem","crystal","wedge","shield","dome","arch","cloud","teardrop","leaf"];
const r=new Function('m1t',src+';return {Yo,cgt,hte,td,TA,Hke,Ue};')(m1t);
const poses={neutral:null,tilt_l:{turn:9,tilt:-8,roll:22},tilt_r:{turn:27,tilt:-20,roll:36},glance_l:{turn:5,tilt:-14,roll:29},glance_r:{turn:31,tilt:-14,roll:29}};
const out={center:r.Ue,viewbox:[-15,-15,259,259],colors:{},shapes:{}};
for(const c of Object.keys(r.Hke)) out.colors[c]=r.Hke[c];
for(const s of m1t){const ta=r.TA(s);const e={scale:ta.scale,body:r.Yo[s].path,eyes:{}};
 for(const [n,p] of Object.entries(poses)){const o=p?{...ta,pose:p}:ta;const [c,u]=r.cgt(s,o);const [l,rr]=r.td[0];e.eyes[n]=[r.hte(l,c),r.hte(rr,u)];}
 out.shapes[s]=e;}
fs.writeFileSync(process.argv[3],JSON.stringify(out));
