### Bisa — the desktop: addons (`desktop/src/addons`, 18 — Addons). A manifest's
### own words — an addon's name, its description — are the developer's content and
### never translated; every sentence around them is here.

## The bridge's refusals (`addonBridgeModel.mjs`): what an addon hears when a call fails a rule.
addons-addon-bridge-refused = the platform refused this
addons-addon-bridge-refused-not-ready = the platform has not said it is ready yet
addons-addon-bridge-refused-unknown-method = the platform offers no such method
addons-addon-bridge-refused-permission = the person did not grant this addon that permission
addons-addon-bridge-refused-not-allowed = the manifest does not allow this window that move
addons-addon-bridge-refused-bad-params = the parameters are not the shape this method takes
addons-addon-bridge-refused-too-large = the message is larger than the platform reads
addons-addon-bridge-refused-rate-limited = too soon after the last one
addons-addon-bridge-refused-quota = this addon's storage is full
addons-addon-bridge-refused-unavailable = the platform did not answer

## The window (`AddonWindow.tsx`).
addons-addon-window-move = Move { $name }
addons-addon-window-close = Close { $name }
addons-addon-window-resize = Resize { $name }

## The layer's one question (`AddonLayer.tsx`): a link an addon wants opened.
addons-addon-layer-open-link-title = { $name } wants to open a link in your browser
addons-addon-layer-open-link = Open in the browser

## The shared words (`addonsModel.mjs`).
addons-addons-model-no-addons = No addons
addons-addons-model-showing-of = { $showing } of { $total } addons showing
addons-addons-model-show-addons = Show addons
addons-addons-model-hint-on = Every running addon floats over the app; ⌘⇧X hides them all
addons-addons-model-hint-off = Off, every window is hidden; what each addon is and what it was granted is kept
addons-addons-model-origin-catalog = built in
addons-addons-model-origin-imported = imported
addons-addons-model-state-running = running
addons-addons-model-state-off = off
addons-addons-model-state-put-away = put away
addons-addons-model-state-files-missing = files not on this machine
addons-addons-model-permission-platform-info = Read the platform's version and your language
addons-addons-model-permission-theme = Read the theme's scheme, and hear when it changes
addons-addons-model-permission-system-load = Read the machine's load — CPU, GPU, memory, disk
addons-addons-model-permission-workspace-summary = Read how many things wait on you, are under review, or are running
addons-addons-model-permission-notify = Show you a notice, at most one every ten seconds
addons-addons-model-permission-clipboard-write = Put text on the clipboard
addons-addons-model-permission-storage = Keep a small store of its own on this machine
addons-addons-model-permission-network = Fetch from the internet, only these hosts, only through the platform: { $hosts }
addons-addons-model-permission-open-url = Ask you to open a link in your browser
addons-addons-model-permission-navigate = Take you to one of the app's screens
addons-addons-model-permission-unknown = Something the platform does not know
addons-addons-model-review-title = Install { $name }?
addons-addons-model-review-asks-nothing = It asks for nothing: it can only draw itself
addons-addons-model-review-asks-for = { $n ->
    [one] It asks for one thing:
   *[other] It asks for { $n } things:
  }
addons-addons-model-version-licence = Version { $version } · licence { $license }
