const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');

const appUrl = process.env.DCFC_UI_CHECK_URL || 'http://127.0.0.1:1420';
const evidence = path.resolve(__dirname, '../artifacts/ui-status-072');
fs.mkdirSync(evidence, { recursive: true });

async function installMock(page, mode, versionFails = false) {
  await page.addInitScript(({mode, versionFails}) => {
    const settings = {
      network_mode: mode, proxy_host: '127.0.0.1', proxy_port: 7877,
      mcp_proxy_host: '127.0.0.1', mcp_proxy_port: 8100, router_port: 8101,
      health_host: '127.0.0.1', health_port: 8080, profile_name: '', tunnel_id: 'tunnel_test',
      public_base_url: '', mcp_executable: 'C:\\DCFC\\runtime\\mcp.exe',
      proxy_executable: 'C:\\DCFC\\runtime\\mcp-proxy\\mcp-proxy.exe',
      proxy_config_path: 'C:\\Test\\proxy.toml', router_config_path: 'C:\\Test\\router.json',
      tunnel_executable: 'C:\\Test\\tunnel-client.exe',
      projects: [{id:'screencast',name:'ScreenCast',output_directory:'C:\\Test\\docs',mcp_host:'127.0.0.1',mcp_port:8000,enabled:true}],
      active_project_id: 'screencast',
    };
    const status = {
      overall:'stopped',proxy_ready:true,router_ready:false,mcp_proxy_ready:false,mcp_ready:false,tunnel_ready:false,
      proxy_pid:null,router_pid:null,mcp_pid:null,tunnel_pid:null,credential_configured:false,
      text_editing_available:true,connector_capability_message:'已支持',message:'未启动',projects:[],
    };
    window.__uiStatusCalls = [];
    window.__uiStatusOriginalProjects = structuredClone(settings.projects);
    window.__uiStatusSavedSettings = settings;
    window.__TAURI_INTERNALS__ = {
      callbacks:new Map(), transformCallback(callback){const id=this.callbacks.size+1;this.callbacks.set(id,callback);return id;},
      unregisterCallback(id){this.callbacks.delete(id);}, convertFileSrc(value){return value;},
      async invoke(command, args = {}) {
        window.__uiStatusCalls.push(command);
        if (command === 'plugin:app|version') {
          if (versionFails) throw new Error('version IPC unavailable');
          return '9.4.2';
        }
        if (command === 'get_settings') return structuredClone(settings);
        if (command === 'get_status') return structuredClone(status);
        if (command === 'get_runtime_readiness') return {mcp_available:true,proxy_available:true,tunnel_available:true,credential_configured:false};
        if (command === 'save_settings') {Object.assign(settings, structuredClone(args.settings));return structuredClone(settings);}
        if (command.includes('plugin:event')) return 1;
        throw new Error(`Unexpected IPC ${command}`);
      },
    };
  }, {mode, versionFails});
}

async function checkMode(page, mode) {
  const top = page.locator('.conn-infra .ic').first();
  const chain = page.locator('.path-flow .path-step').first();
  const title = mode === 'direct' ? 'Direct' : 'Magic';
  assert.equal((await top.locator('.ic-name').innerText()).trim(), title);
  assert.equal((await chain.locator('.pname').innerText()).trim(), title);
  if (mode === 'direct') {
    assert.match(await top.innerText(), /不依赖本机代理/);
    assert.match(await chain.innerText(), /不依赖本机代理/);
    assert.doesNotMatch(await top.innerText(), /7877|Magic Proxy/);
  } else {
    assert.match(await top.innerText(), /127\.0\.0\.1:7877/);
    assert.match(await chain.innerText(), /127\.0\.0\.1:7877/);
  }
}

(async () => {
  const browser = await chromium.launch({channel:'msedge',headless:true});
  const results = [];
  try {
    for (const width of [1180,900]) {
      for (const initialMode of ['direct','magic']) {
        const page = await browser.newPage({viewport:{width,height:850}});
        page.setDefaultTimeout(7000);
        await installMock(page,initialMode);
        await page.goto(appUrl,{waitUntil:'domcontentloaded'});
        await page.locator('.sidebar-version').filter({hasText:'v9.4.2'}).waitFor({state:'attached'});
        await page.getByText('ScreenCast',{exact:true}).first().waitFor();
        await checkMode(page,initialMode);
        await page.screenshot({path:path.join(evidence,`${initialMode}-${width}.png`),fullPage:true});
        const otherMode = initialMode === 'direct' ? 'magic' : 'direct';
        await page.getByRole('button',{name:'设置',exact:true}).click();
        await page.getByRole('button',{name:otherMode === 'direct' ? 'Direct':'Magic',exact:true}).click();
        await page.getByRole('button',{name:'保存设置',exact:true}).click();
        await page.getByText('设置已保存。启用项目后可以同时启动。',{exact:true}).waitFor();
        await page.getByRole('button',{name:'首页',exact:true}).click();
        await checkMode(page,otherMode);
        const state = await page.evaluate(() => ({calls:window.__uiStatusCalls,projects:window.__uiStatusSavedSettings.projects,original:window.__uiStatusOriginalProjects}));
        assert.deepEqual(state.projects,state.original);
        assert(state.calls.includes('plugin:app|version'));
        assert(!state.calls.some(c=>/^(start|stop|save_runtime_key)/.test(c)));
        assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
        results.push({width,initialMode,switchedTo:otherMode,status:'passed'});
        await page.close();
      }
      const page = await browser.newPage({viewport:{width,height:850}});
      await installMock(page,'direct',true);
      await page.goto(appUrl,{waitUntil:'domcontentloaded'});
      await page.locator('.sidebar-version').filter({hasText:'版本未知'}).waitFor({state:'attached'});
      assert.doesNotMatch(await page.locator('.sidebar-version').innerText(), /v1\.0\.0|v0\.1/);
      await checkMode(page,'direct');
      results.push({width,versionFailure:'passed'});
      await page.close();
    }
    fs.writeFileSync(path.join(evidence,'result.json'),JSON.stringify({status:'passed',source:'mocked IPC UI regression, not live runtime acceptance',results},null,2));
    console.log(JSON.stringify(results,null,2));
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
