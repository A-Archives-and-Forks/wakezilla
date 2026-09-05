// Run inside the Playwright container; every API request is intercepted.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const base = process.env.DASHBOARD_URL;
assert.ok(base, 'Set DASHBOARD_URL to the LAN HTTP URL of the running dashboard');
const output = process.env.DASHBOARD_OUTPUT || '/out';
fs.mkdirSync(output, { recursive: true });

const initialMachines = () => ['NAS','Mac Studio','Gaming PC','Raspberry Pi','Homelab','ThinkPad'].map((name,index) => ({
  name, mac: `02:00:00:00:00:0${index+1}`, ip: `192.168.1.${10+index}`,
  machine_type:['nas','mini_pc','computer','raspberry_pi','server','notebook'][index],
  description:'Test machine', can_be_turned_off:true, turn_off_port:3001, inactivity_period:0,
  port_forwards:[{name:index===0?'Plex':'SSH',local_port:32000+index,target_port:index===0?32400:22}],
}));

async function fixture(page) {
  const state = { machines:initialMachines(), statuses:new Map(), setup:new Map(), calls:[], reachable:false, failCreate:false, failUpdate:false, failList:false, failStatus:false, rotation:0 };
  for (const [index,machine] of state.machines.entries()) { state.statuses.set(machine.mac, ![2,5].includes(index)); state.setup.set(machine.mac,'verified'); }
  const setupFor = mac => state.setup.get(mac)==='verified' ? {status:'verified'} : {
    status:'pending', unix_command:`sudo wakezilla setup --mode client --port 3001 --key TEST_ONLY_KEY_${state.rotation} --yes`,
    windows_command:`wakezilla setup --mode client --port 3001 --key TEST_ONLY_KEY_${state.rotation} --yes`,
  };
  await page.route('**/api/**', async route => {
    const request=route.request(), url=new URL(request.url()), path=decodeURIComponent(url.pathname), method=request.method();
    assert.equal(url.origin,new URL(base).origin,'API must use the page origin');
    state.calls.push({path,method});
    const send=(json,status=200)=>route.fulfill({status,json});
    if(path==='/api/machines'&&method==='GET') return state.failList ? send({error:'Test load failure'},500) : send(state.machines);
    if(path==='/api/machines'&&method==='POST') {
      if(state.failCreate) return send({errors:{mac:['MAC already registered']}},400);
      const machine=request.postDataJSON(); state.machines.unshift(machine); state.setup.set(machine.mac,'pending'); state.statuses.set(machine.mac,false); return send({status:'Machine added'},201);
    }
    if(path==='/api/interfaces') return send([{name:'eth-test',ip:'192.168.1.17',mac:'02:00:00:00:01:01',is_up:true}]);
    if(path==='/api/scan') { assert.equal(url.searchParams.get('interface'),'eth-test'); return send([{ip:'192.168.1.90',mac:'02:00:00:00:00:90',hostname:'Test discovery'}]); }
    if(path==='/api/machines/delete') { const {mac}=request.postDataJSON(); state.machines=state.machines.filter(machine=>machine.mac!==mac); return send({status:'Machine deleted'}); }
    const match=path.match(/^\/api\/machines\/([^/]+)(.*)$/);
    if(match) {
      const [,mac,action]=match;
      if(!action&&method==='GET') return send(state.machines.find(machine=>machine.mac===mac));
      if(!action&&method==='PUT') { if(state.failUpdate) return send({error:'Test save failure'},500); const value=request.postDataJSON(); state.machines=state.machines.map(machine=>machine.mac===mac?value:machine); return send({status:'Machine updated'}); }
      if(action==='/is-on') return state.failStatus&&mac.endsWith('01') ? send({error:'Test failure'},500) : send({is_on:state.statuses.get(mac)||false});
      if(action==='/shutdown-setup') return send(setupFor(mac));
      if(action==='/shutdown-setup/verify') { if(state.reachable) {state.setup.set(mac,'verified'); return send({status:'verified'});} return send({...setupFor(mac),status:'unreachable'}); }
      if(action==='/shutdown-setup/rotate') {state.rotation++;state.setup.set(mac,'pending');return send(setupFor(mac));}
      if(action==='/wake'||action==='/remote-turn-off') return send({message:'Command accepted by test fixture'});
      if(action==='/access-history') return send({services:[{name:'Plex',local_port:32000,target_port:32400,timestamps:[Date.now()-60000,Date.now()-120000]}]});
    }
    throw new Error(`Unexpected API operation: ${method} ${path}`);
  });
  return state;
}
async function noOverflow(page) {
  const sizes=await page.evaluate(()=>({width:innerWidth,page:document.documentElement.scrollWidth,dialog:document.querySelector('dialog[open]')?.clientWidth,scroll:document.querySelector('dialog[open]')?.scrollWidth}));
  assert.ok(sizes.page<=sizes.width,JSON.stringify(sizes));
  if(sizes.dialog) assert.ok(sizes.scroll<=sizes.dialog+1,JSON.stringify(sizes));
}
async function close(page) { await page.getByRole('button',{name:'Close window',exact:true}).click(); await page.locator('dialog').waitFor({state:'hidden'}); }
async function openMachine(page,name) { await page.getByRole('button',{name:`Open ${name}`,exact:true}).click(); await page.locator('#machine-name').waitFor(); }
async function addFields(page,name='Backup server',mac='02:00:00:00:00:88') {
  await page.locator('#add-button').click(); await page.locator('#machine-name').fill(name); await page.locator('#machine-ip').fill('192.168.1.88'); await page.locator('#machine-mac').fill(mac); await page.locator('#machine-type').selectOption('nas');
}
async function submit(page) { await page.locator('#machine-form button[type=submit]').click(); }
async function waitText(locator,text) { await locator.filter({hasText:text}).waitFor(); }

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.PLAYWRIGHT_CHROMIUM_PATH||chromium.executablePath(),args:['--no-sandbox']});
 try {
  for(const width of [1440,390,320]) {
   const context=await browser.newContext({viewport:{width,height:width===1440?1000:844},locale:'en-US',timezoneId:'America/Sao_Paulo'});
   const page=await context.newPage(); page.setDefaultTimeout(15000);
   const errors=[];page.on('pageerror',error=>errors.push(error.message));
   const state=await fixture(page);
   if(width===1440) state.failList=true;
   await page.goto(base);
   assert.equal(await page.locator('html').getAttribute('lang'),'en');
   assert.equal(await page.locator('.brand').getAttribute('href'),'https://wakezilla.dev');
   if(state.failList) {await waitText(page.locator('[role=alert]'),'Could not load');state.failList=false;await page.getByRole('button',{name:'Try again'}).click();}
   await page.getByRole('button',{name:'Open NAS',exact:true}).waitFor();
   await page.locator('.machine-card.online').first().waitFor();
   assert.equal(await page.locator('.clock-block').isVisible(),width>760);
   for(const theme of ['dark','light']) {
    const current=await page.locator('html').getAttribute('data-theme');if(current!==theme) await page.locator('#theme-toggle').click();
    // Wait for the card background transition before checking the theme.
    await page.waitForFunction(theme => {
      const card=document.querySelector('.machine-card');
      const red=Number(getComputedStyle(card).backgroundColor.match(/[\d.]+/)[0]);
      return theme==='light' ? red>240 : red<100;
    },theme);
    await noOverflow(page);await page.screenshot({path:`${output}/dashboard-${width}-${theme}.png`,fullPage:true,animations:'disabled'});
    await openMachine(page,'NAS');await noOverflow(page);
    const surfaces=await page.evaluate(()=>({dialog:getComputedStyle(document.querySelector('dialog')).backgroundColor,input:getComputedStyle(document.querySelector('#machine-name')).backgroundColor}));
    assert.equal(surfaces.dialog,theme==='light'?'rgb(243, 246, 240)':'rgb(30, 43, 42)');
    assert.equal(surfaces.input,theme==='light'?'rgb(252, 253, 249)':'rgba(8, 20, 20, 0.25)');
    const inactivity=page.locator('#machine-idle');await inactivity.fill('17');await inactivity.fill('');assert.equal(await inactivity.inputValue(),'');assert.equal(await inactivity.evaluate(input=>input.checkValidity()),false);await inactivity.fill('0');
    await page.screenshot({path:`${output}/edit-${width}-${theme}.png`,animations:'disabled'});await close(page);
   }
   await page.getByRole('button',{name:'List view'}).click();await noOverflow(page);await page.getByRole('button',{name:'Grid view'}).click();
   await page.getByRole('searchbox').fill('Plex');assert.equal(await page.locator('.machine-card').count(),1);await page.getByRole('searchbox').fill('');
   await addFields(page);
   if(width===1440) {state.failCreate=true;await submit(page);await waitText(page.locator('.form-error'),'MAC already registered');assert.equal(state.machines.length,6);state.failCreate=false;}
   await submit(page);await page.getByRole('button',{name:'Check connection',exact:true}).waitFor();
   assert.equal(state.machines[0].machine_type,'nas');assert.equal(state.machines[0].can_be_turned_off,true);assert.equal(state.machines[0].turn_off_port,3001);assert.equal(state.machines[0].inactivity_period,60);
   await page.getByRole('button',{name:'Windows',exact:true}).click();await waitText(page.locator('.setup-command').first(),'irm');
   await page.getByRole('button',{name:'Linux / macOS',exact:true}).click();await waitText(page.locator('.setup-command').first(),'curl');
   await page.getByRole('button',{name:'Copy command: 2. Set up the client',exact:true}).click();await waitText(page.locator('.setup-command').nth(1),'Command copied.');
   assert.equal(await page.evaluate(()=>window.isSecureContext),false,'Use a LAN HTTP URL to cover clipboard fallback');
   await noOverflow(page);await page.locator('dialog').evaluate(element=>{element.scrollTop=0});await page.screenshot({path:`${output}/setup-${width}.png`});
   await page.getByRole('button',{name:'Set up later',exact:true}).click();
   await openMachine(page,'Backup server');await waitText(page.locator('.client-setup-summary'),'Setup pending');
   await page.locator('#machine-description').fill('Draft retained');
   await page.locator('.client-setup-summary').getByRole('button',{name:'Set up client'}).click();
   state.reachable=true;await page.getByRole('button',{name:'Check connection',exact:true}).click();await waitText(page.locator('.setup-success'),'Client configured');
   await page.getByRole('button',{name:'View machine',exact:true}).click();assert.equal(await page.locator('#machine-description').inputValue(),'Draft retained');
   await page.getByRole('button',{name:'Add service',exact:true}).click();await page.locator('[id^=service-name-]').fill('SSH');await page.locator('[id^=service-local-]').fill('32222');await page.locator('[id^=service-target-]').fill('22');
   await page.locator('#machine-name').fill('Updated backup');
   if(width===1440) {state.failUpdate=true;await submit(page);await waitText(page.locator('.form-error'),'Test save failure');assert.equal(state.machines[0].name,'Backup server');assert.equal(await page.locator('#machine-name').inputValue(),'Updated backup');state.failUpdate=false;}
   await submit(page);await waitText(page.locator('.edit-feedback'),'Changes saved.');assert.equal(state.machines[0].port_forwards[0].local_port,32222);
   await page.getByRole('button',{name:'Access history',exact:true}).click();await page.locator('.history-table').waitFor();await page.getByRole('button',{name:'Week',exact:true}).click();await noOverflow(page);await page.getByRole('button',{name:'Back to machine',exact:true}).click();
   await close(page);await page.getByRole('button',{name:'Open Updated backup',exact:true}).waitFor();
   if(width===1440) {
    state.failStatus=true;await waitText(page.locator('.machine-card').filter({has:page.getByRole('button',{name:'Open NAS',exact:true})}),'Status unavailable');state.failStatus=false;
    await openMachine(page,'NAS');await waitText(page.locator('.detail-top'),'Online');await page.getByRole('button',{name:'Shut down',exact:true}).click();await page.getByRole('button',{name:'Back',exact:true}).click();assert.equal(state.calls.filter(call=>call.path.endsWith('/remote-turn-off')).length,0);
    await page.getByRole('button',{name:'Shut down',exact:true}).click();await page.getByRole('button',{name:'Shut down machine',exact:true}).click();await waitText(page.locator('.detail-top'),'Shutting down');state.statuses.set('02:00:00:00:00:01',false);await waitText(page.locator('.detail-top'),'Unreachable');await close(page);
    const gaming=page.locator('.machine-card').filter({has:page.getByRole('button',{name:'Open Gaming PC',exact:true})});
    await gaming.getByRole('button',{name:'Wake machine',exact:true}).click();await waitText(gaming,'Waking');assert.equal(await gaming.locator('.machine-status').textContent(),'Waking...');state.statuses.set('02:00:00:00:00:03',true);await waitText(gaming.locator('.machine-status'),'Online');
    await openMachine(page,'Updated backup');await page.locator('.client-setup-summary').getByRole('button',{name:'Set up client'}).click();await page.getByRole('button',{name:'Set up client again',exact:true}).click();assert.equal(state.rotation,0);state.reachable=false;await page.getByRole('button',{name:'Generate new key',exact:true}).click();await waitText(page.locator('.setup-command').nth(1),'TEST_ONLY_KEY_1');assert.equal(state.rotation,1);await close(page);
    await page.getByRole('button',{name:'Find on network',exact:true}).click();await page.getByRole('button',{name:'Find devices',exact:true}).click();await page.locator('.discover-card').getByRole('button',{name:'Add',exact:true}).click();assert.equal(await page.locator('#machine-mac').inputValue(),'02:00:00:00:00:90');await close(page);
    await openMachine(page,'Updated backup');await page.getByRole('button',{name:'Delete machine',exact:true}).click();await page.getByRole('button',{name:'Cancel',exact:true}).click();assert.equal(state.machines.length,7);await page.getByRole('button',{name:'Delete machine',exact:true}).click();await page.getByRole('button',{name:'Delete machine',exact:true}).click();await page.locator('dialog').waitFor({state:'hidden'});assert.equal(state.machines.length,6);
    await page.goto(`${base}/machines/02%3A00%3A00%3A00%3A00%3A01`);await page.locator('#machine-name').waitFor();assert.equal(await page.locator('#machine-name').inputValue(),'NAS');await page.keyboard.press('Escape');await page.locator('dialog').waitFor({state:'hidden'});
   }
   await page.reload();assert.equal(await page.locator('html').getAttribute('data-theme'),'light');
   assert.deepEqual(errors,[]);console.log(JSON.stringify({width,result:'passed',coverage:'themes, layout, CRUD, setup, LAN clipboard, draft, services, history',requests:state.calls.length}));
   await context.close();
  }
 } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
