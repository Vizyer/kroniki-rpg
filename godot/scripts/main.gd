extends Control

const BG := Color("16181c")
const PANEL := Color("20242a")
const GOLD := Color("c4a96b")
const TEXT := Color("ded7c8")
const MUTED := Color("8f938f")
const RED := Color("b75b57")

var core:CoreClient
var narration:RichTextLabel
var input:TextEdit
var suggestions:VBoxContainer
var left_panel:VBoxContainer
var right_panel:VBoxContainer
var status:Label
var cancel_button:Button
var save_id := -1
var pending_action_id := -1
var state:Dictionary = {}

func _ready() -> void:
    set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
    _build_ui()
    core = CoreClient.new()
    add_child(core)
    core.core_ready.connect(_on_core_ready)
    core.request_completed.connect(_on_core_response)
    core.request_cancelled.connect(_on_request_cancelled)

func _build_ui() -> void:
    var root := VBoxContainer.new()
    root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
    root.add_theme_constant_override("separation", 0)
    add_child(root)

    var top := PanelContainer.new()
    top.custom_minimum_size.y = 56
    var top_style := StyleBoxFlat.new()
    top_style.bg_color = PANEL
    top_style.border_width_bottom = 1
    top_style.border_color = Color("3a3f46")
    top.add_theme_stylebox_override("panel", top_style)
    root.add_child(top)

    var th := HBoxContainer.new()
    th.add_theme_constant_override("separation", 14)
    top.add_child(th)

    var title := Label.new()
    title.text = "KRONIKI RPG"
    title.add_theme_color_override("font_color", GOLD)
    title.add_theme_font_size_override("font_size", 22)
    th.add_child(title)

    status = Label.new()
    status.text = "Łączenie z rdzeniem…"
    status.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    status.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
    status.add_theme_color_override("font_color", MUTED)
    th.add_child(status)
    th.add_child(_button("ZAPISZ", _save_game))
    th.add_child(_button("WCZYTAJ", _load_dialog))

    var body := HBoxContainer.new()
    body.size_flags_vertical = Control.SIZE_EXPAND_FILL
    body.add_theme_constant_override("separation", 1)
    root.add_child(body)

    left_panel = VBoxContainer.new()
    left_panel.custom_minimum_size.x = 250
    left_panel.add_theme_constant_override("separation", 8)
    body.add_child(_wrap_panel(left_panel, PANEL))

    var center := VBoxContainer.new()
    center.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    center.add_theme_constant_override("separation", 10)
    body.add_child(_wrap_panel(center, BG))

    narration = RichTextLabel.new()
    narration.bbcode_enabled = true
    narration.fit_content = false
    narration.scroll_active = true
    narration.size_flags_vertical = Control.SIZE_EXPAND_FILL
    narration.add_theme_color_override("default_color", TEXT)
    narration.add_theme_font_size_override("normal_font_size", 18)
    narration.text = "[color=#8f938f]Mistrz Gry budzi świat…[/color]"
    center.add_child(narration)

    suggestions = VBoxContainer.new()
    suggestions.add_theme_constant_override("separation", 5)
    center.add_child(suggestions)

    var action_row := HBoxContainer.new()
    action_row.custom_minimum_size.y = 96
    action_row.add_theme_constant_override("separation", 8)
    center.add_child(action_row)

    input = TextEdit.new()
    input.placeholder_text = "Co robi, mówi albo próbuje zrobić twoja postać?"
    input.size_flags_horizontal = Control.SIZE_EXPAND_FILL
    input.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
    action_row.add_child(input)

    var send := _button("DZIAŁAJ", _submit_action)
    send.custom_minimum_size.x = 112
    action_row.add_child(send)

    cancel_button = _button("ANULUJ", _cancel_action)
    cancel_button.visible = false
    action_row.add_child(cancel_button)

    right_panel = VBoxContainer.new()
    right_panel.custom_minimum_size.x = 280
    right_panel.add_theme_constant_override("separation", 8)
    body.add_child(_wrap_panel(right_panel, PANEL))

    _refresh_sidebars({})

func _wrap_panel(content:Control, color:Color) -> PanelContainer:
    var p := PanelContainer.new()
    var s := StyleBoxFlat.new()
    s.bg_color = color
    s.content_margin_left = 16
    s.content_margin_right = 16
    s.content_margin_top = 14
    s.content_margin_bottom = 14
    p.add_theme_stylebox_override("panel", s)
    p.add_child(content)
    return p

func _button(text_value:String, callback:Callable) -> Button:
    var b := Button.new()
    b.text = text_value
    b.pressed.connect(callback)
    return b

func _label(text_value:String, size:int = 14, color:Color = TEXT) -> Label:
    var l := Label.new()
    l.text = text_value
    l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
    l.add_theme_font_size_override("font_size", size)
    l.add_theme_color_override("font_color", color)
    return l

func _clear(node:Node) -> void:
    for child in node.get_children():
        child.queue_free()

func _on_core_ready(info:Dictionary) -> void:
    status.text = "Rdzeń %s • schema %s" % [str(info.get("core", "?")), str(info.get("schema", "?"))]
    core.request("/state", {}, "GET")

func _submit_action() -> void:
    var text := input.text.strip_edges()
    if text == "" or pending_action_id > 0:
        return
    input.text = ""
    narration.append_text("\n\n[color=#c4a96b]› %s[/color]\n" % text)
    status.text = "Mistrz Gry rozstrzyga…"
    pending_action_id = core.request("/action", {"text": text, "mode": "freeform"})
    cancel_button.visible = true

func _cancel_action() -> void:
    if pending_action_id > 0:
        core.cancel(pending_action_id)

func _on_request_cancelled(request_id:int) -> void:
    if request_id == pending_action_id:
        pending_action_id = -1
        cancel_button.visible = false
        status.text = "Odpowiedź anulowana. Akcja nie została zatwierdzona."

func _on_core_response(route:String, ok:bool, data:Dictionary) -> void:
    if route == "/state" and ok:
        state = data
        _refresh_sidebars(data)
        return

    if route == "/action":
        pending_action_id = -1
        cancel_button.visible = false
        if not ok:
            status.text = "MGAI nie odpowiedział — stan gry nie został uszkodzony."
            narration.append_text("\n[color=#b75b57]Rdzeń nie mógł zatwierdzić akcji.[/color]")
            return
        state = data.get("state", {})
        var source := str(data.get("source", "model"))
        status.text = "MGAI • %s" % ("lokalny fallback" if source == "local_fallback" else "model lokalny")
        var text := str(data.get("narration", ""))
        narration.append_text("\n\n%s" % text)
        _set_suggestions(data.get("suggestions", []))
        _refresh_sidebars(state)
        return

    if route == "/save":
        if ok:
            save_id = int(data.get("id", save_id))
            status.text = "Gra zapisana."
        else:
            status.text = "Błąd zapisu: %s" % str(data.get("error", "nieznany"))
        return

    if route == "/saves" and ok:
        _show_load_list(data.get("saves", []))
        return

    if route == "/save/load" and ok:
        save_id = int(data.get("id", -1))
        state = data.get("state", {})
        narration.text = str(state.get("last_narration", "Wczytano kampanię."))
        _set_suggestions(state.get("last_suggestions", []))
        _refresh_sidebars(state)
        status.text = "Wczytano zapis."

func _set_suggestions(items:Array) -> void:
    _clear(suggestions)
    for item in items.slice(0, 5):
        var text := str(item)
        var b := _button(text, func():
            input.text = text
            _submit_action()
        )
        suggestions.add_child(b)

func _refresh_sidebars(s:Dictionary) -> void:
    _clear(left_panel)
    _clear(right_panel)

    left_panel.add_child(_label("BOHATER", 13, GOLD))
    var ch:Dictionary = s.get("character", {}) if s.has("character") else {}
    left_panel.add_child(_label(str(ch.get("name", "Nieznany")), 22))
    left_panel.add_child(_label(
        "HP %s   STA %s   VIG %s" % [
            str(ch.get("hp", "—")),
            str(ch.get("stamina", "—")),
            str(ch.get("vigor", "—"))
        ],
        14,
        MUTED
    ))
    left_panel.add_child(_label("Chaos: %s" % str(ch.get("chaos", "—")), 14, MUTED))

    right_panel.add_child(_label("SCENA", 13, GOLD))
    var w:Dictionary = s.get("world", {}) if s.has("world") else {}
    right_panel.add_child(_label(str(w.get("location", "Nieznane miejsce")), 18))

    var clock:Dictionary = w.get("clock", {}) if w.has("clock") else {}
    if not clock.is_empty():
        right_panel.add_child(_label(
            "%02d:%02d • %02d.%02d.%04d" % [
                int(clock.get("hour",0)),
                int(clock.get("minute",0)),
                int(clock.get("day",1)),
                int(clock.get("month",1)),
                int(clock.get("year",1272))
            ],
            13,
            MUTED
        ))

    right_panel.add_child(_label(str(w.get("weather", "")), 13, MUTED))

    if s.has("combat") and bool(s.combat.get("active", false)):
        right_panel.add_child(_label("PULS STARCIA", 13, RED))
        right_panel.add_child(_label(
            "Tempo %s • Guard %s • Pressure %s" % [
                str(s.combat.get("tempo",0)),
                str(s.combat.get("guard",0)),
                str(s.combat.get("pressure",0))
            ],
            13
        ))

func _save_game() -> void:
    var default_name := "Zapis"
    if state.has("character"):
        default_name = str(state.character.get("name", "Postać")) + " — " + str(state.get("world", {}).get("location", "Zapis"))
    var payload := {"name": default_name}
    if save_id > 0:
        payload["id"] = save_id
    core.request("/save", payload)

func _load_dialog() -> void:
    core.request("/saves", {}, "GET")

func _show_load_list(saves:Array) -> void:
    if saves.is_empty():
        status.text = "Brak zapisów."
        return
    _clear(suggestions)
    for save_row in saves.slice(0, 8):
        var id := int(save_row.get("id", -1))
        var name := str(save_row.get("name", "Zapis"))
        var b := _button("WCZYTAJ • " + name, func():
            core.request("/save/load", {"id": id})
        )
        suggestions.add_child(b)
