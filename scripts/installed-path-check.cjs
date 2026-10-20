const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const { chromium } = require('playwright');

// Run against an extracted/installed application, with an isolated Windows profile.
const executable = path.resolve(process.argv[2] || '');
const evidence = path.resolve(process.argv[3] || 'artifacts/installed-path-check');
assert.equal(path.basename(executable), 'delegate-control.exe');
assert(fs.existsSync(executable), 'Pass the installed/extracted delegate-control.exe path');
const install = path.dirname(executable);
const normalize = value => value.replace(/^\\\\\?\\/, '').replaceAll('/', '\\').toLowerCase();
const expected = {
  mcp_executable: path.join(install, 'runtime/chatgpt-delegate-edit/chatgpt-delegate-edit.exe'),
  proxy_executable: path.join(install, 'runtime/mcp-proxy/mcp-proxy.exe'),
  tunnel_executable: path.join(install, 'runtime/tunnel-client/tunnel-client.exe'),
};
const tunnelVersion = spawnSync(expected.tunnel_executable, ['--version'], {windowsHide:true, encoding:'utf8'});
assert.equal(tunnelVersion.status, 0, tunnelVersion.stderr);
assert.match(tunnelVersion.stdout + tunnelVersion.stderr, /0\.0\.14/);
const capabilities = spawnSync(expected.mcp_executable, ['capabilities', '--json'], {windowsHide:true, encoding:'utf8'});
assert.equal(capabilities.status, 0, capabilities.stderr);
assert.equal(JSON.parse(capabilities.stdout).connector_version, '0.2.0-dcfc-router.2');

async function runCase(name, previous) {
  const home = path.join(evidence, name, '测试 用户');
  fs.mkdirSync(home, { recursive: true });
  const root = path.join(home, '.delegate-control');
  if (previous) {
    fs.mkdirSync(root, { recursive: true });
    fs.writeFileSync(path.join(root, 'settings.json'), JSON.stringify(previous));
  }
  const port = name === 'fresh' ? 9238 : 9239;
  const child = spawn(executable, [], { windowsHide: true, stdio: 'ignore', env: {
    ...process.env, USERPROFILE: home, APPDATA: path.join(home, 'AppData/Roaming'), LOCALAPPDATA: path.join(home, 'AppData/Local'),
    WEBVIEW2_USER_DATA_FOLDER: path.join(home, 'webview'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  } });
  let browser;
  try {
    for (let i = 0; ; i++) {
      if (child.exitCode !== null) throw new Error(`Application exited: ${child.exitCode}`);
      try { const response = await fetch(`http://127.0.0.1:${port}/json/version`); if (response.ok) break; } catch {}
      if (i >= 120) throw new Error('Installed WebView CDP not ready');
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    let page;
    for (let i = 0; i < 60; i++) {
      page = browser.contexts().flatMap(context => context.pages()).find(candidate => /tauri\.localhost/.test(candidate.url()));
      if (page) break;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    assert(page, 'Installed Tauri page missing');
    await page.waitForFunction(() => Boolean(window.__TAURI_INTERNALS__?.invoke));
    const invoke = (command, args = {}) => page.evaluate(({command, args}) => window.__TAURI_INTERNALS__.invoke(command, args), {command, args});
    const settings = await invoke('get_settings');
    for (const [field, value] of Object.entries(expected)) {
      assert.equal(normalize(settings[field]), normalize(value));
      assert(fs.existsSync(settings[field]), `Displayed executable missing: ${field}`);
    }
    const readiness = await invoke('get_runtime_readiness');
    for (const field of ['mcp_available', 'proxy_available', 'tunnel_available']) assert.equal(readiness[field], true);
    if (previous) {
      assert.deepEqual(settings.projects, previous.projects);
      assert.equal(settings.active_project_id, previous.active_project_id);
      const persisted = JSON.parse(fs.readFileSync(path.join(root, 'settings.json'), 'utf8'));
      for (const field of Object.keys(expected)) assert.equal(persisted[field], settings[field]);
    } else assert.equal(settings.projects.length, 0);

    const testSettings = { ...settings, network_mode: 'direct', projects: Array.from({length:12}, (_, index) => ({
      id:`test-${index}`, name:`测试项目 ${index}`, output_directory:path.join(home, `文档 ${index}`), mcp_host:'127.0.0.1', mcp_port:8500+index, enabled:true,
    })), active_project_id:'test-0' };
    await invoke('save_settings', {settings:testSettings});
    const projectDir = path.join(root, 'logs/test-11');
    fs.mkdirSync(projectDir, {recursive:true});
    fs.writeFileSync(path.join(projectDir, 'mcp.stdout.log'), 'PROJECT-11-LOG-MARKER');
    assert.equal(normalize(await invoke('get_log_directory', {source:'mcp',projectId:'test-11'})), normalize(projectDir));
    assert.match(await invoke('read_logs', {source:'mcp',projectId:'test-11'}), /PROJECT-11-LOG-MARKER/);
    await invoke('open_log_directory', {source:'mcp',projectId:'test-11'});
    const shellCheck = spawnSync('pwsh', ['-NoProfile', '-Command', `
      $shell = New-Object -ComObject Shell.Application
      for ($attempt=0; $attempt -lt 40; $attempt++) {
        foreach ($window in $shell.Windows()) {
          try { if ($window.Document.Folder.Self.Path -eq $env:DCFC_EXPECTED_LOG_DIRECTORY) {
            Write-Output 'EXPLORER_TARGET_MATCH'; $window.Quit(); exit 0
          } } catch {}
        }
        Start-Sleep -Milliseconds 250
      }
      Write-Error 'Explorer did not open the displayed project log directory'; exit 1
    `], {windowsHide:true, encoding:'utf8', env:{...process.env, DCFC_EXPECTED_LOG_DIRECTORY:projectDir}});
    assert.equal(shellCheck.status, 0, shellCheck.stderr);
    assert.match(shellCheck.stdout, /EXPLORER_TARGET_MATCH/);
    for (const source of ['router', 'proxy', 'tunnel', 'startup']) {
      assert.equal(normalize(await invoke('get_log_directory', {source,projectId:null})), normalize(path.join(root,'logs')));
    }
    await assert.rejects(invoke('get_log_directory', {source:'mcp',projectId:'not-configured'}));
    console.log(JSON.stringify({case:name, pid:child.pid, executable, paths:expected, projects:12, status:'passed'}));
    return testSettings;
  } finally {
    if (browser) await browser.close();
    if (child.exitCode === null) spawnSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], {windowsHide:true,stdio:'ignore'});
  }
}

(async () => {
  const initial = await runCase('fresh');
  await runCase('upgrade', { ...initial,
    mcp_executable:'C:\\Old Install\\runtime\\chatgpt-delegate-edit\\chatgpt-delegate-edit.exe',
    proxy_executable:'C:\\Old Install\\runtime\\mcp-proxy\\mcp-proxy.exe',
    tunnel_executable:'C:\\Old Install\\runtime\\tunnel-client\\tunnel-client.exe',
  });
})().catch(error => { console.error(error); process.exitCode=1; });
