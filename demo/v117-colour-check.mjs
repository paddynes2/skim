import {chromium} from 'playwright';
import {writeFileSync,readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
const helper=process.env.SKIM_PALETTE_HELPER??'C:/Users/Patrick/OS/tools/design-corpus/palette.mjs';
const {collectPalette,auditPalette}=await import(pathToFileURL(helper));
const browser=await chromium.launch(),results=[];
try {
 for(const theme of ['cold-light','cold-dark','warm-light','warm-dark','base-light','base-dark']) {
  const page=await browser.newPage({viewport:{width:1600,height:1000}});
  await page.addInitScript(theme=>localStorage.setItem('skimdemo.theme',theme),theme);
  await page.goto(process.env.SKIM_DEMO_URL??'http://127.0.0.1:1431/');await page.locator('.sidebar .item',{hasText:'Deals'}).click();
  await page.evaluate(()=>document.fonts.ready);
  const snapshot=await page.evaluate(collectPalette,[{name:'meeting accent',foreground:'--meeting-ink',background:'--meeting-soft',minimum:4.5},{name:'company accent',foreground:'--deal-ink',background:'--deal-soft',minimum:4.5}]);
  // Canvas converts supported browser paint, including color-mix's color(srgb),
  // to an explicit sRGB pixel for the shared auditor. Keep the original paint.
  const normalized=await page.evaluate(snapshot=>{
   const ctx=document.createElement('canvas').getContext('2d',{willReadFrequently:true});
   const rgb=value=>{ctx.clearRect(0,0,1,1);ctx.fillStyle=value;ctx.fillRect(0,0,1,1);const [r,g,b,a]=ctx.getImageData(0,0,1,1).data;return `rgba(${r}, ${g}, ${b}, ${a/255})`;};
   for(const item of [...snapshot.text,...snapshot.tokens]){item.originalColour=item.colour;item.originalBackgrounds=item.backgrounds;item.colour=rgb(item.colour);item.backgrounds=item.backgrounds.map(rgb);}
   return snapshot;
  },snapshot);
  const audit=auditPalette(normalized);results.push(audit);assert.ok(audit.tokens.every(t=>t.status==='pass'),JSON.stringify(audit.tokens));
  console.log(theme,audit.tokens.map(t=>`${t.name}: ${t.ratio.toFixed(2)}`).join(', '));await page.close();
 }
 writeFileSync('docs/fork/shots/1.1.7/colour-evidence.json',JSON.stringify({helperSha256:createHash('sha256').update(readFileSync(helper)).digest('hex'),method:'Computed browser colours normalized through a canvas sRGB pixel. Original computed strings retained. All visible text recorded; assertions cover the two new accent pairs.',results},null,2)+'\n');
}finally{await browser.close();}
