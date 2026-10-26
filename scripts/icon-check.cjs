const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');

const root = path.resolve(__dirname, '..');
const version = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version;
const url = process.env.DCFC_UI_CHECK_URL || 'http://127.0.0.1:1432';
const ico = fs.readFileSync(path.join(root, 'src-tauri/icons/icon.ico'));
assert.equal(ico.readUInt16LE(2), 1);
const sizes = Array.from({length: ico.readUInt16LE(4)}, (_, i) => ico[6 + i * 16] || 256);
assert(sizes.includes(32) && sizes.includes(256), 'Windows ICO must include small and large sizes');

(async () => {
  const browser = await chromium.launch({channel:'msedge', headless:true});
  const evidence = path.join(root, 'artifacts/icon-20261026');
  fs.mkdirSync(evidence, {recursive:true});
  try {
    for (const width of [1180, 900]) {
      const page = await browser.newPage({viewport:{width, height:850}});
      await page.addInitScript((version) => {
        window.__TAURI_INTERNALS__ = {async invoke(command) {
          if (command === 'plugin:app|version') return version;
          if (command === 'get_settings') return {
            projects:[], active_project_id:null, network_mode:'direct',
            proxy_host:'127.0.0.1', proxy_port:7877, mcp_proxy_host:'127.0.0.1',
            mcp_proxy_port:8100, router_port:8101, health_host:'127.0.0.1', health_port:8080,
            profile_name:'', tunnel_id:'', public_base_url:'', mcp_executable:'C:\\Test\\mcp.exe',
            proxy_executable:'C:\\Test\\proxy.exe', tunnel_executable:'C:\\Test\\tunnel.exe',
            proxy_config_path:'C:\\Test\\proxy.toml', router_config_path:'C:\\Test\\router.json',
          };
          if (command.includes('runtime_readiness')) return {mcp_available:true, proxy_available:true, tunnel_available:true, credential_configured:false};
          if (command === 'get_status') return {
            overall:'stopped', projects:[], credential_configured:false, text_editing_available:true,
            connector_capability_message:'Test fixture', message:'Test fixture',
          };
          throw new Error(`Unexpected IPC in read-only icon test: ${command}`);
        }};
      }, version);
      await page.goto(url);
      const wizard = page.locator('.wizard-brand .brand-mark img');
      await wizard.waitFor();
      for (const selector of ['.brand-lockup .brand-mark img', '.wizard-brand .brand-mark img']) {
        const result = await page.locator(selector).evaluate(async (img) => {
          await img.decode();
          const canvas = document.createElement('canvas');
          canvas.width = img.naturalWidth; canvas.height = img.naturalHeight;
          const ctx = canvas.getContext('2d'); ctx.drawImage(img, 0, 0);
          const pixel = (x,y) => [...ctx.getImageData(x,y,1,1).data];
          return {w:img.naturalWidth, h:img.naturalHeight, corner:pixel(0,0), center:pixel(512,512),
            css:img.getBoundingClientRect().toJSON(), background:getComputedStyle(img.parentElement).backgroundColor};
        });
        assert.equal(result.w, 1024); assert.equal(result.h, 1024);
        assert.equal(result.corner[3], 0, 'Screenshot background must be transparent');
        assert(result.center[3] >= 254, 'Icon body must be opaque');
        assert.equal(result.css.width, 32); assert.equal(result.css.height, 32);
        assert.equal(result.background, 'rgba(0, 0, 0, 0)');
      }
      const favicon = await page.locator('link[rel="icon"]').getAttribute('href');
      assert.match(favicon, /dcfc-icon/);
      await page.screenshot({path:path.join(evidence, `wizard-${width}.png`)});
      await page.getByRole('button', {name:'稍后配置', exact:true}).first().click();
      await page.screenshot({path:path.join(evidence, `dashboard-${width}.png`)});
      await page.close();
    }
    console.log(JSON.stringify({status:'passed', icoSizes:sizes, viewports:[1180,900], evidence,
      scope:'Mocked IPC icon rendering only; no runtime configuration or installed client changed'}));
  } finally { await browser.close(); }
})().catch(error => {console.error(error); process.exitCode = 1;});
