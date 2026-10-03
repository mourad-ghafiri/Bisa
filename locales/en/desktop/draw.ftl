### Bisa — the desktop: Draw (`desktop/src/draw`, 19 — Drawings).

draw-activity-agent-drawing = An agent is drawing
draw-activity-agents-drawing = { $count } agents are drawing
draw-canvas-menu-note = Saved as you draw. Commit from the panel's strip.
draw-dock-drawings = Drawings
draw-dock-drawings-count = Drawings ({ $count })
draw-editor-ask-agent = Ask an agent
draw-editor-back-to-list = Back to the list
draw-editor-could-not-delete = could not delete
draw-editor-could-not-load-canvas = the canvas could not be loaded
draw-editor-could-not-re-read = could not re-read the drawing
draw-editor-could-not-rename = could not rename
draw-editor-could-not-save = could not save
draw-editor-delete = Delete this drawing
draw-editor-hide-ask = Hide the conversation
draw-editor-loading-canvas = Opening the canvas…
draw-editor-somebody-drew-meanwhile = Somebody drew here meanwhile
draw-editor-strokes-since-are-lost = Somebody else drew here while you were drawing. Taking theirs replaces the canvas; your strokes since your last save are lost.
draw-editor-take-theirs = Take theirs
draw-editor-title = Drawing title
draw-library-actor = Person
draw-library-api-gateway = API gateway
draw-library-badge-blocked = Blocked badge
draw-library-badge-done = Done badge
draw-library-badge-idea = Idea badge
draw-library-badge-question = Question badge
draw-library-cache = Cache
draw-library-callout = Callout
draw-library-checkmark = Checkmark
draw-library-client = Client
draw-library-database = Database
draw-library-external-system = External system
draw-library-legend = Legend
draw-library-load-balancer = Load balancer
draw-library-note-card = Note card
draw-library-queue = Queue
draw-library-section-frame = Section frame
draw-library-service = Service
draw-library-sticky-blue = Sticky note, blue
draw-library-sticky-green = Sticky note, green
draw-library-sticky-pink = Sticky note, pink
draw-library-sticky-violet = Sticky note, violet
draw-library-sticky-yellow = Sticky note, yellow
draw-library-title-banner = Title banner
draw-library-warning = Warning
draw-overlay-clear-search = Clear the search
draw-overlay-close = Close drawings
draw-overlay-could-not-create = The drawing could not be created. The diagnostic log has the detail.
draw-overlay-could-not-delete = The drawing could not be deleted.
draw-overlay-delete-drawing = Delete “{ $title }”
draw-overlay-could-not-load = The drawings could not be read. The diagnostic log has the detail.
draw-overlay-elements = { $count ->
    [one] 1 element
   *[other] { $count } elements
}
draw-overlay-here = { $t } (here)
draw-overlay-new = New
draw-overlay-new-drawing = New drawing
draw-overlay-no-channel-drawing-yet = No drawing about a channel yet.
draw-overlay-no-drawing-named-so = No drawing is named so.
draw-overlay-no-drawings-yet = No drawings yet
draw-overlay-no-goal-drawing-yet = No drawing about a goal yet.
draw-overlay-no-node-drawing-yet = No drawing about this node yet.
draw-overlay-no-project-drawing-yet = No drawing about a project yet.
draw-overlay-no-workflow-drawing-yet = No drawing about a workflow yet.
draw-overlay-no-workspace-drawings-yet = No workspace drawings yet — the ones about nothing in particular.
draw-overlay-nothing-here-yet = Nothing here yet. A drawing travels to everyone in the workspace, and an agent can draw into it.
draw-overlay-nothing-to-file-under-yet = Nothing to file a drawing under yet
draw-overlay-panel-height = Drawings panel height
draw-overlay-panel-width = Drawings panel width
draw-overlay-pinned = Pinned
draw-overlay-search = Search drawings
draw-overlay-untitled-drawing = Untitled drawing
draw-settings-any-agent-may-draw = Any agent may draw into a drawing through the drawing tools.
draw-settings-drawing-tools-refuse-every-agent = The drawing tools refuse every agent, the platform's own included.
draw-settings-only-agents-carrying-drawing-skill = Only agents that carry the Drawing skill — the platform's own agents always may.
draw-status-saving = Saving…
draw-status-unsaved = Unsaved
draw-template-empty = Empty
draw-template-entity-relationship = Entities and relations
draw-template-flowchart = Flowchart
draw-template-kanban = Board
draw-template-mind-map = Mind map
draw-template-retrospective = Retrospective
draw-template-swimlanes = Swimlanes
draw-template-system-architecture = System architecture
draw-template-timeline = Timeline
draw-template-user-journey = User journey
draw-template-wireframe = Wireframe
draw-request-drawing-gone = the drawing is not there any more
draw-request-drawing-moved = the drawing changed under the canvas twice; read it and draw again
draw-request-not-a-skeleton = `elements` is not a list of skeleton elements
draw-request-mermaid-refused = the Mermaid text could not be laid out
draw-request-snapshot-failed = the snapshot could not be rendered
draw-request-unknown-action = the canvas does not perform this action
draw-tpl-system-architecture = System architecture
draw-tpl-web-client = Web client
draw-tpl-api-gateway = API gateway
draw-tpl-orders-service = Orders service
draw-tpl-users-service = Users service
draw-tpl-orders-db = orders db
draw-tpl-do-the-thing = Do the thing
draw-tpl-fix-it = Fix it
draw-tpl-the-idea = The idea
draw-tpl-mind-map = Mind map
draw-tpl-to-do = To do
draw-tpl-user-journey = User journey
draw-tpl-what-they-feel = What they feel
draw-tpl-entity-customer = Customer
    id · name · email
draw-tpl-entity-order = Order
    id · customer_id · total
draw-tpl-entity-order-line = Order line
    order_id · sku · qty
draw-tpl-entity-product = Product
    sku · name · price
draw-tpl-went-well = Went well
draw-tpl-could-be-better = Could be better
draw-tpl-try-next = Try next
draw-tpl-body-copy = Body copy…
draw-tpl-primary-action = Primary action
draw-tpl-what-this-screen-is-for = What this screen is for
draw-shape-api-gateway = API gateway
draw-shape-load-balancer = Load balancer
draw-shape-external-system = External system
draw-shape-say-it-here = Say it here
draw-shape-note-card-text = Heading

    A line or two of words.
draw-tpl-card = Card { $n }
draw-tpl-discover = Discover
draw-tpl-sign-up = Sign up
draw-tpl-first-use = First use
draw-tpl-return = Return
draw-tpl-recommend = Recommend

## The conversation drawer beside the canvas (`views/_studio/ConversationDrawer.tsx`).
draw-editor-ask-width = Conversation drawer width
draw-editor-ask-hint = Ask an agent to draw here — the picture lands on the canvas beside you as it draws.

## *New drawing* — the dialog (`draw/NewDrawingDialog.tsx`): the gallery, a title, where it is filed.
draw-new-description = Pick a picture to start from, name it, and say where it is filed.
draw-new-template = Start from
draw-new-name = Name
draw-new-where = Where
draw-new-create = Create
draw-new-cancel = Cancel
draw-template-empty-blurb = A blank canvas — draw anything.
draw-template-system-architecture-blurb = The parts of a system and how they talk: clients, a gateway, services, stores.
draw-template-flowchart-blurb = Steps and a decision, start to done.
draw-template-swimlanes-blurb = One lane per actor, with each one's steps in it.
draw-template-mind-map-blurb = An idea in the middle and its branches around it.
draw-template-kanban-blurb = Three columns of cards: to do, doing, done.
draw-template-user-journey-blurb = What a person does, step by step, and what they feel at each.
draw-template-entity-relationship-blurb = The things a system stores and how they relate.
draw-template-retrospective-blurb = Went well, could be better, try next — sticky notes in three columns.
draw-template-wireframe-blurb = A screen's frame: header, body, a button, a footer.
draw-template-timeline-blurb = Milestones on a line, kick-off to review.

## The words the templates draw with, continued (`draw/templates/index.mjs`).
draw-tpl-cache = cache
draw-tpl-backend = Backend
draw-tpl-start = Start
draw-tpl-worked = Worked?
draw-tpl-done = Done
draw-tpl-doing = Doing
draw-tpl-yes = yes
draw-tpl-no = no
draw-tpl-why = Why
draw-tpl-what = What
draw-tpl-who = Who
draw-tpl-when = When
draw-tpl-customer = Customer
draw-tpl-support = Support
draw-tpl-engineering = Engineering
draw-tpl-lane-step = { $lane } step { $n }
draw-tpl-screen-title = Title
draw-tpl-image = Image
draw-tpl-headline = Headline
draw-tpl-kick-off = Kick-off
draw-tpl-alpha = Alpha
draw-tpl-beta = Beta
draw-tpl-launch = Launch
draw-tpl-review = Review

## Save and Delete (`draw/DrawEditor.tsx`); a list row's Delete asks in the same words (`draw/DrawOverlay.tsx`).
draw-editor-save = Save
draw-editor-delete-title = Delete “{ $title }”?
draw-editor-delete-body = The drawing, its snapshot and its conversations are gone. The drawings repository records the deletion at your next commit.
draw-editor-delete-confirm = Delete

## The drawing bridge's answer when a skeleton cannot be laid out (`draw/drawBridge.ts`) — words moved out of the code.
draw-bridge-skeleton-could-not-be-laid-out = the skeleton could not be laid out: { $why }
