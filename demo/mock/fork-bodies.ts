// Fork (Phase 5): rendered-body fixtures with quote and signature markers, in
// the shape the Rust sanitizer emits (only `skim-quote` / `skim-sig`,
// `divRplyFwdMsg` / `appendonsend`, `blockquote type="cite"` survive it).
//
// Served by `demo/mock/tauri-core.ts` for `get_message_body` when the
// localStorage flag `skimdemo.fork_body` is `gmail`, `outlook` or `plain`
// (wired by the main session; see docs/fork/pending/5.md "## tauri-core mock").
import { renderedBody } from "./data";

// Gmail reply: own words, Gmail signature block, then the quoted thread under
// its attribution line, exactly as Gmail's HTML is structured after sanitising.
export const GMAIL_QUOTED_BODY = `
<div dir="ltr">Thanks Anna, that works. I will take the final pass on the landing copy by Wednesday EOD and send it back with tracked changes.</div>
<div dir="ltr"><br></div>
<div dir="ltr">One thing: the pricing toggle needs the web team before Thursday, so I have pinged Marcus on that thread.</div>
<div dir="ltr"><br></div>
<div class="skim-sig" dir="ltr">
  <div dir="ltr">Alex Rivera<br>Product · Skim<br>+1 415 555 0137</div>
</div>
<div class="skim-quote">
  <div dir="ltr">On Tue, 23 Sep 2026 at 09:14, Anna Kowalski &lt;anna@example.com&gt; wrote:<br></div>
  <blockquote type="cite" style="margin:0 0 0 .8ex;border-left:1px #ccc solid;padding-left:1ex">
    <div dir="ltr">Hi Alex,<br><br>The Q3 launch is Thursday. Three items still need an owner: the landing page copy (due Wednesday), the pricing page toggle change, and the launch email.<br><br>Can you take the final pass on the landing copy?<br><br>Anna</div>
    <div class="skim-quote">
      <div dir="ltr">On Mon, 22 Sep 2026 at 17:02, Alex Rivera &lt;alex@example.com&gt; wrote:<br></div>
      <blockquote type="cite" style="margin:0 0 0 .8ex;border-left:1px #ccc solid;padding-left:1ex">
        <div dir="ltr">Anna, what is still open for Thursday?</div>
      </blockquote>
    </div>
  </blockquote>
</div>
`;

// Outlook reply: own words, then the `appendonsend` anchor, a rule and the
// `divRplyFwdMsg` header block followed by the quoted body as siblings (the
// folded stylesheet hides `#divRplyFwdMsg` and everything after it).
export const OUTLOOK_QUOTED_BODY = `
<div dir="ltr">
  <div style="font-family:Aptos,sans-serif;font-size:12pt">Hi Alex,</div>
  <div style="font-family:Aptos,sans-serif;font-size:12pt"><br></div>
  <div style="font-family:Aptos,sans-serif;font-size:12pt">Confirmed for Thursday. Legal signed off on the MSA v3 this morning; the redlines are attached.</div>
  <div style="font-family:Aptos,sans-serif;font-size:12pt"><br></div>
  <div style="font-family:Aptos,sans-serif;font-size:12pt">Best,<br>Priya</div>
</div>
<div id="appendonsend"></div>
<hr style="display:inline-block;width:98%">
<div id="divRplyFwdMsg" dir="ltr">
  <font face="Calibri, sans-serif" color="#000000" style="font-size:11pt"><b>From:</b> Alex Rivera &lt;alex@example.com&gt;<br><b>Sent:</b> Tuesday, 23 September 2026 08:40<br><b>To:</b> Priya Nair &lt;priya@example.com&gt;<br><b>Subject:</b> Re: Acme MSA v3</font>
  <div>&nbsp;</div>
</div>
<div dir="ltr">
  <div style="font-family:Aptos,sans-serif;font-size:12pt">Priya, are we still on for Thursday? Anything outstanding on the MSA?</div>
</div>
<div id="x_appendonsend"></div>
<div dir="ltr">
  <div style="font-family:Aptos,sans-serif;font-size:12pt"><b>From:</b> Priya Nair<br><b>Sent:</b> Monday, 22 September 2026 15:10<br><b>Subject:</b> Acme MSA v3</div>
  <div style="font-family:Aptos,sans-serif;font-size:12pt">Sending v3 for review.</div>
</div>
`;

// Plain text as `text_to_html` renders it: the sender's words in one <pre>,
// the tail (signature delimiter onward) wrapped in `skim-quote`.
export const PLAIN_QUOTED_BODY = `<pre class="skim-plain">Sounds good, see you Thursday.

Marcus
</pre><div class="skim-quote"><pre class="skim-plain">--
Marcus Lee
Web · Skim

On Tue, 23 Sep 2026, Alex Rivera wrote:
&gt; Marcus, can the pricing toggle ship before Thursday?
&gt; Anna needs it for the launch.</pre></div>`;

// A lone forwarded message: nothing of the sender's own above the header, so
// the Rust side reports `hasFold: false` and the pane shows the forward whole.
export const OUTLOOK_FORWARD_BODY = `
<div dir="ltr"><div style="font-family:Aptos,sans-serif;font-size:12pt"><br></div></div>
<div id="appendonsend"></div>
<hr style="display:inline-block;width:98%">
<div id="divRplyFwdMsg" dir="ltr">
  <font face="Calibri, sans-serif" color="#000000" style="font-size:11pt"><b>From:</b> Anna Kowalski &lt;anna@example.com&gt;<br><b>Sent:</b> Tuesday, 23 September 2026 09:14<br><b>Subject:</b> Launch owners</font>
  <div>&nbsp;</div>
</div>
<div dir="ltr"><div style="font-family:Aptos,sans-serif;font-size:12pt">Three items still need an owner for Thursday.</div></div>
`;

export type ForkBodyKind = "gmail" | "outlook" | "plain" | "forward";

const BODIES: Record<ForkBodyKind, { html: string; hasFold: boolean }> = {
  gmail: { html: GMAIL_QUOTED_BODY, hasFold: true },
  outlook: { html: OUTLOOK_QUOTED_BODY, hasFold: true },
  plain: { html: PLAIN_QUOTED_BODY, hasFold: true },
  forward: { html: OUTLOOK_FORWARD_BODY, hasFold: false },
};

/** `renderedBody()` with the html swapped for a quoted fixture. */
export function forkRenderedBody(messageId: number, kind: ForkBodyKind) {
  return { ...renderedBody(messageId), ...BODIES[kind] };
}
