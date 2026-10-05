# Privacy policy

This page describes ReShiki's application and project website. Updated
2026-10-05. ReShiki is maintained by
[@Ameyanagi](https://github.com/Ameyanagi); its source and documentation are
available in the [project repository](https://github.com/Ameyanagi/ReShiki).

## Local drawing and chemistry

Drawing, file imports and exports, clipboard operations, molecular properties,
structure checks, cleanup, and 3D calculations run on your computer. They do not
upload your drawings for processing. Chemistry workers run from the same
application executable, without downloading a runtime or chemistry environment.

Drawings you save, recovery drafts, templates, and preferences can be stored
locally. ReShiki does not provide a drawing-upload or cloud-sync service. Sharing
a saved file, exported figure, or clipboard content with another application is
your choice; that application's handling of it is separate from ReShiki.

ReShiki does not include an automatic usage-analytics or crash-report uploader.
The network features below are separate from local drawing and calculations.

## Update checks

**Automatic update checks are enabled by default.** After startup and when a
cached result expires, ReShiki checks the public GitHub releases API. Stable is
the default channel; you can choose Nightly. Successful results are normally
cached for 24 hours and failed checks for one hour.

The request identifies the ReShiki version in its User-Agent. Like other network
requests, GitHub can receive your IP address and request metadata. Update checks
do not send drawings, clipboard contents, assistant prompts, or signing
credentials. Choosing a download or **Update and restart** also contacts GitHub
and its release-download infrastructure.

To disable automatic checks, open **ReShiki → Check for updates** and turn off
**Check automatically**. Manual checks remain available. Automatic checks never
install an update without your action. The current Windows installer does not
offer this preference during installation; it is available in the application.

GitHub handles requests under its
[privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
See the [installation and update guide](https://reshiki.com/guide/install/#updates)
for channel and download behavior.

## Optional Codex assistant

The assistant is optional. Opening or connecting it uses your locally installed
Codex and account, including account and model discovery. Sending a drawing
request uses Codex's configured model service and requires internet access.

Model requests can include your prompt, conversation context, the current drawing,
selection, styles, rendered canvas images, and any source image you attach.
The current drawing and rendered canvas can include embedded pictures. The
assistant is not limited to inspecting only selected atoms. An additional model
review can receive the generated drawing and source image and make further
requests.

ReShiki limits this assistant session to its drawing tools, with shell execution,
web search, external apps, and additional agents disabled. These limits do not
make model processing local. Your Codex account, provider, and data settings
determine how the model service handles the data. For OpenAI, consult
[OpenAI's privacy policy](https://openai.com/policies/privacy-policy/). If you
configure another provider, consult that provider's privacy policy as well.

Ordinary drawing and chemistry do not require connecting the assistant. Read
[Set up Codex](assistant-setup.md#what-happens-when-you-send-a-request) before
sending a request or image.

## Optional Office integrations

ReShiki's native Office integration uses local files and interprocess
communication. A separately installed Microsoft 365 task pane also loads
Microsoft-hosted Office.js and connects to the local ReShiki companion. Office's
account services and cloud saving remain controlled by Office. Consult
[Microsoft's privacy statement](https://privacy.microsoft.com/en-us/privacystatement)
and the [Office integration guide](../integrations/office/README.md) when using
that optional integration.

## Website and external links

The project website is published through GitHub Pages. Its homepage requests the
public GitHub releases API to display the latest stable version. The current
website does not add a separate usage-analytics tracker. GitHub's hosting and API
privacy practices apply to these requests.

Links to documentation, sponsors, release downloads, and other sites open those
services in your browser. Their privacy policies apply when you visit them.
Choosing to report a problem on GitHub publishes the information you submit
according to that issue's visibility.

## Questions and changes

Contact the maintainer through the
[project repository](https://github.com/Ameyanagi/ReShiki) with questions about
this policy. Changes to network features or data handling should update this page
alongside the implementation.
