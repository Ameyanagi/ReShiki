// node prefs.mjs <url> <out>: change appearance, save a template, a theme and
// an abbreviation, then reload and look for each.
import { start } from './lib.mjs';
const [url, out] = process.argv.slice(2);
const s = await start(out);
const { page, wait, shot, say } = s;
const NAME_Y = Number(process.env.NAME_Y ?? 388);
const [DEFINE_X, DEFINE_Y] = (process.env.DEFINE ?? '1120,560').split(',').map(Number);
await s.open(url);
say('stored at start:', await s.stored());

// 1. Dark appearance.
await page.mouse.click(640, 520);
await s.palette('Appearance: Dark');
await shot('01-dark-mode');

// 3. Name the phenyl ring as an abbreviation.
await page.evaluate(() => window.editor.load('c1ccccc1O', 'smiles'));
await wait(800);
await page.mouse.move(270, 290);
await page.mouse.down();
await page.mouse.move(430, 410, { steps: 5 });
await page.mouse.move(545, 520, { steps: 5 });
await page.mouse.up();
await wait(400);
await s.palette('Abbreviations panel');
await page.mouse.click(1152, 591); // Label
await page.keyboard.type('Phx', { delay: 15 });
await wait(300);
await page.mouse.move(1150, 450);
await page.mouse.wheel(0, 200);
await wait(600);
await shot('02-abbreviation-form');
await page.mouse.click(DEFINE_X, DEFINE_Y); // Define abbreviation from selection
await wait(1200);
await shot('03-abbreviation-saved');

// 4. A theme from the theme generator.
await page.mouse.click(900, 300);
await s.palette('Theme generator panel');
await page.mouse.click(1099, 175); // Manage themes…
await wait(1200);
await page.mouse.click(1108, 454); // Pastel
await wait(500);
await page.mouse.click(1189, 31); // Save
await wait(1500);
await shot('04-theme-saved');
await page.mouse.click(76, 31); // Back to drawing
await wait(800);
await shot('05-theme-applied');
// 2. Save the selection as a template.
await page.mouse.click(900, 300);
await page.evaluate(() => window.editor.load('c1ccccc1O', 'smiles'));
await wait(800);
await page.mouse.click(640, 520);
await page.keyboard.press('Meta+a');
await wait(400);
await s.palette('Templates panel');
await page.mouse.click(1152, 269); // Save selection as template
await wait(600);
await page.mouse.click(1152, NAME_Y); // Template name
await page.keyboard.type('Phenol (mine)', { delay: 15 });
await page.mouse.click(1062, NAME_Y + 75); // Save
await wait(1200);
await shot('06-template-saved');

say('stored before reload:', await s.stored());

// 5. Reload.
await s.reload();
say('stored after reload:', await s.stored());
await shot('07-after-reload');
await page.evaluate(() => window.editor.load('c1ccccc1', 'smiles'));
await wait(800);
await page.mouse.click(640, 520);
await s.palette('Templates panel');
await page.mouse.click(1152, 196); // Search
await page.keyboard.type('mine', { delay: 15 });
await wait(800);
await shot('08-template-after-reload');
await page.mouse.click(900, 300);
await page.mouse.click(943, 691); // color theme menu in the status bar
await wait(800);
await shot('09-themes-after-reload');
await page.keyboard.press('Escape');
await wait(300);
await page.mouse.click(900, 300);
await s.palette('Phx', { pick: false });
await shot('10-abbreviation-after-reload');
await s.finish();
