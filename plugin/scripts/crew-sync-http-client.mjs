#!/usr/bin/env node
// Host-native CrewSync MCP transport. Credentials arrive through BUS_TOKEN in the environment.
// No Docker exec, no proxying outside loopback, no logging secrets or request payloads.
const endpoint = 'http://127.0.0.1:18787/mcp';
const token = process.env.BUS_TOKEN;
const sessionEnv = process.env.BUS_SESSION || '';
if (!token) { console.error('CREWSYNC_AUTH_MISSING'); process.exit(125); }
let args = process.argv.slice(2);
const sessionAt = args.indexOf('--session');
let session = sessionEnv;
if (sessionAt >= 0) { session = args[sessionAt+1] || ''; args.splice(sessionAt,2); }
const jsonAt = args.indexOf('--json');
if (jsonAt >= 0) args.splice(jsonAt,1);
const flag = (name, fallback) => {
 const i=args.indexOf('--'+name);
 if(i<0) return fallback;
 if(i===args.length-1 || args[i+1].startsWith('--')) { args.splice(i,1); return true; }
 const v=args.splice(i,2)[1]; return v;
};
const compact=o=>Object.fromEntries(Object.entries(o).filter(([,v])=>v!==undefined&&v!==null));
const numberOrUndefined=x=>x===undefined?undefined:Number(x);
const kind=args.shift(), sub=(['task','lock','note'].includes(kind)?args.shift():undefined);
let tool,payload={},allPages=false;
if(kind==='whoami') tool='whoami';
else if(kind==='tools') tool='__tools__';
else if(kind==='call'){tool=args.shift();try {payload=JSON.parse(flag('args','{}'));}catch{throw Error('INVALID_CALL_ARGS');}}
else if(kind==='task'){
 const key=args.shift();
 const lease=numberOrUndefined(flag('lease-seconds',undefined));
 const value=flag('result',undefined);
 const title=flag('title',undefined),description=flag('description',undefined),depends=flag('depends-on',undefined);
 const mapper={show:'get_task',update:'update_task',claim:'claim_task',renew:'renew_task_lease',release:'release_task',done:'complete_task',next:'claim_next_task',create:'create_task'};
 tool=mapper[sub];if(!tool)throw Error('UNSUPPORTED_TASK_COMMAND:'+sub);
 if(sub==='update'){
   const expected=flag('expected-revision',undefined);
   if(expected===undefined || !/^[1-9][0-9]*$/.test(String(expected)))throw Error('INVALID_EXPECTED_REVISION');
   const clearDescription=flag('clear-description',false)===true;
   const clearMetadata=flag('clear-metadata',false)===true;
   const rawMetadata=flag('metadata-json',undefined);
   if(clearDescription && description!==undefined)throw Error('CONFLICTING_DESCRIPTION_OPTIONS');
   if(clearMetadata && rawMetadata!==undefined)throw Error('CONFLICTING_METADATA_OPTIONS');
   payload={key,expected_revision:Number(expected)};
   if(title!==undefined)payload.title=title;
   if(clearDescription)payload.description=null;
   else if(description!==undefined)payload.description=description;
   if(clearMetadata)payload.metadata=null;
   else if(rawMetadata!==undefined){
      try{payload.metadata=JSON.parse(rawMetadata);}catch{throw Error('INVALID_METADATA_JSON');}
   }
   if(args.length>0)throw Error('UNKNOWN_UPDATE_FLAGS:'+args.join(','));
 }else payload=compact({key:sub==='next'?undefined:key,lease_seconds:lease,result:value,title,description,depends_on:depends?.split(',')});
}
else if(kind==='tasks'){
 tool='list_tasks';allPages=flag('all-pages',false)===true;
 payload=compact({status:flag('status',undefined),mine_only:flag('mine',false)===true,project_prefix:flag('project-prefix',undefined),limit:numberOrUndefined(flag('limit',50)),cursor:flag('cursor',undefined),search:flag('search',undefined),search_mode:flag('search-mode',undefined),search_fields:flag('search-fields',undefined),search_language:flag('search-language',undefined)});
}
else if(kind==='lock'){
 const mapper={list:'list_locks',acquire:'acquire_lock',release:'release_lock'};tool=mapper[sub];
 payload=compact({name:args.shift(),ttl_seconds:numberOrUndefined(flag('ttl-seconds',undefined)),purpose:flag('purpose',undefined)});
}
else if(kind==='note'){
 const mapper={get:'get_note',set:'set_note',rm:'delete_note',search:'search_notes'};tool=mapper[sub];
 const key=args.shift();
 payload=compact({key:sub==='search'?undefined:key,query:sub==='search'?key:undefined,scope:flag('scope',undefined),value:flag('value',undefined),tags:flag('tags',undefined)?.split(',')});
}
else if(kind==='notes'){tool='list_notes';payload=compact({scope:flag('scope',undefined),tag:flag('tag',undefined)});}
else if(kind==='sessions'){tool='list_sessions';payload=compact({project:flag('project',undefined),role:flag('role',undefined),online_only:flag('online',false)===true,limit:numberOrUndefined(flag('limit',undefined))});}
else if(kind==='agents'){tool='list_agents';payload={online_only:flag('online',false)===true};}
else if(kind==='read'){tool='read_messages';payload=compact({scope:flag('scope','all'),only_new:flag('history',false)!==true,limit:numberOrUndefined(flag('limit',50)),all_sessions:flag('all-sessions',false)===true});}
else if(kind==='send'){tool='post_message';payload=compact({channel:flag('channel',undefined),to:flag('to',undefined),body:flag('body',undefined),announce:flag('announce',false)===true,reply_to:numberOrUndefined(flag('reply-to',undefined))});}
else throw Error('CREWSYNC_COMMAND_UNSUPPORTED:'+kind);
if(!tool)throw Error('CREWSYNC_COMMAND_UNSUPPORTED');
let id=0, sessionId='';
const baseHeaders={'accept':'application/json, text/event-stream','content-type':'application/json','mcp-protocol-version':'2025-06-18','authorization':'Bearer '+token};
if(session)baseHeaders['X-Crew-Session']=session;
async function rpc(method,params={},notification=false){
 const headers={...baseHeaders};if(sessionId)headers['Mcp-Session-Id']=sessionId;
 let response;try{
 response=await fetch(endpoint,{method:'POST',headers,signal:AbortSignal.timeout(9000),body:JSON.stringify({jsonrpc:'2.0',...(notification?{}:{id:++id}),method,params})});
 }catch(error){throw Error(error?.name==='TimeoutError'?'CREWSYNC_HTTP_TIMEOUT':'CREWSYNC_HTTP_TRANSPORT:'+String(error?.message).slice(0,100));}
 const sid=response.headers.get('mcp-session-id');if(sid)sessionId=sid;
 const body=await response.text();
 if(!response.ok)throw Error('CREWSYNC_HTTP_'+response.status+':'+body.slice(0,150));
 if(notification||!body.trim())return {};
 let parsed;const lines=body.split(/\r?\n/).filter(x=>x.startsWith('data:'));
 try{parsed=JSON.parse(lines.length?lines.at(-1).slice(5).trim():body);}catch{throw Error('CREWSYNC_INVALID_HTTP_RESPONSE');}
 if(parsed.error)throw Error('CREWSYNC_MCP_ERROR:'+String(parsed.error.message||parsed.error.code));
 return parsed.result||parsed;
}
async function call(tool,args){
 const r=await rpc('tools/call',{name:tool,arguments:args});
 if(r.isError)throw Error('CREWSYNC_TOOL_ERROR:'+JSON.stringify(r.content||[]).slice(0,250));
 return r.structuredContent??r.structured_content??r;
}
try{
 await rpc('initialize',{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'crew-sync-host-native',version:'1.0.0'}});
 await rpc('notifications/initialized',{},true);
 if(tool==='__tools__'){console.log(JSON.stringify((await rpc('tools/list',{})).tools??[],null,2));process.exit(0);}
 let result;
 if(tool==='list_tasks'&&allPages){
  const keys=new Set(),cursors=new Set();let full=[],last;
  for(let i=0;i<10000;i++){
   const page=await call(tool,payload);
   if(!Array.isArray(page.tasks)||typeof page.has_more!=='boolean')throw Error('INCOMPLETE_INVENTORY');
   for(const t of page.tasks){if(!t.key||keys.has(t.key))throw Error('DUPLICATE_OR_INVALID_TASK');keys.add(t.key);full.push(t);}
   last=page;if(!page.has_more){result={...last,tasks:full,exhausted:true,incomplete:false,consistency:'paged_best_effort'};break;}
   if(!page.next_cursor||cursors.has(page.next_cursor))throw Error('INVALID_CURSOR');
   cursors.add(page.next_cursor);payload.cursor=page.next_cursor;
  }
  if(!result)throw Error('INVENTORY_PAGE_LIMIT');
 }else result=await call(tool,payload);
 if(tool==='list_tasks'&&!allPages&&result&&typeof result==='object'){
  const incomplete=result.has_more!==false;result={...result,incomplete,exhausted:!incomplete,pagination:{incomplete,exhausted:!incomplete,consistency:'paged_best_effort'},consistency:'paged_best_effort'};
 }
 console.log(JSON.stringify(result,null,2));
}catch(error){console.error('CREWSYNC_HTTP_FAILURE:',String(error.message||error).slice(0,280));process.exitCode=124;}
