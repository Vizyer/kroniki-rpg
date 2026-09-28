extends Control

const BG = Color("111315")
const PANEL = Color("191d20")
const PANEL_2 = Color("20252a")
const TEXT = Color("e9e1d2")
const MUTED = Color("9fa4a7")
const GOLD = Color("b99a5e")
const RED = Color("994a45")
const GREEN = Color("5f816b")

var core: CoreClient
var state := {
    "character": {"name":"Livia", "profession":"Wiedzmin", "level":1, "hp":31, "max_hp":38, "gold":86,
        "stats":{"STR":3,"DEX":5,"CON":4,"INT":3,"PER":5,"CHA":2},
        "skills":["Miecze","Tropienie","Bestiariusz","Alchemia"],
        "items":["Srebrny miecz","Stalowy miecz","Torba alchemiczna"],
        "knowledge":{"history":1,"politics":1,"geography":2,"monsters":4,"magic":2},
        "magic":{"control":3,"vigor":6,"stamina":10,"chaos_saturation":0,"concentration":100},
        "alchemy":{"skill":4,"prepared":[]},"crafting":{"skill":2}},
    "world":{"location":"Dolny Brod", "year":1272,"month":6,"day":17,"hour":21,"minute":34,"weather":"ulewa",
        "quest":"Zaginieni na trakcie", "mode":"explore"},
    "chronicle":[{"date":"17 VI 1272","title":"Zaginieni na trakcie","text":"Livia przyjela kontrakt dotyczacy zaginiec przy starym trakcie."}],
    "clues":["Slady pazurow przy rowie","Zeznanie Marty: krzyk po zmroku"],
    "hypotheses":["Drapieznik poluje w poblizu wody"],
    "inventory":{"ingredients":{"alkohol":3,"glistnik":4,"mózg utopca":1,"berberka":2,"saletra":2,"fosfor":1,"tłuszcz":2,"jaskółcze ziele":2,"pył kostny":1},"items":[]},
    "npcs":{},"relations":{},"factions":{"lokalna_wladza":{"name":"Lokalna władza","canon":false,"active":true,"resources":45,"influence":35,"clock":0,"public_goal":"utrzymać porządek"}},
    "quests":[],"news_queue":[],"campaign_flags":{},"hunting":{"evidence":[],"hypotheses":[],"confidence":0,"preparation":0},
    "combat":{"active":false,"enemy":"Bruxa","guard":6,"enemy_guard":4,"tempo":1,"pressure":2,"morale":70,"distance":4.0}
}

var center: VBoxContainer
var left_panel: VBoxContainer
var right_panel: VBoxContainer
var bottom_nav: HBoxContainer
var action_input: LineEdit
var status_line: Label
var content_title: Label
var overlay: PanelContainer
var lore_results: VBoxContainer
var knowledge_on := false
var current_tab := "SCENA"
var last_narration := "Deszcz tnie trakt niemal poziomo. Marta stoi pod okapem stajni, zaciskając dłoń na przemoczonym liście. Z lasu za rzeką nie dobiega ani jeden ptasi głos.

Marta: Jeśli Sten naprawdę poszedł w stronę bagien, nie mamy wiele czasu."
var suggested_actions:Array = ["Pytam Martę, czego jeszcze mi nie powiedziała.","Badam list i ślady błota na papierze.","Proszę, by pokazała mi miejsce ostatniego widzenia Stena."]
var pending_request_id := ""
var ai_configured := false
var ai_info := {"mode":"local","profile":"Qwen3-8B Q5_K_M","model_exists":false,"server_exists":false,"ready":false,"last_backend":""}
var save_id := -1
var save_feedback := ""
var save_pending_name := ""

func _ready() -> void:
    set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
    _apply_theme()
    core = CoreClient.new()
    add_child(core)
    core.request_completed.connect(_on_core_response)
    _build_ui()
    core.request("/ai/status", {}, "GET")
    _render_scene()

func _apply_theme() -> void:
    var theme := Theme.new()
    theme.default_font_size = 17
    theme.set_color("font_color","Label",TEXT)
    theme.set_color("font_color","Button",TEXT)
    theme.set_color("font_color","LineEdit",TEXT)
    theme.set_color("font_color","TextEdit",TEXT)
    theme.set_color("font_placeholder_color","LineEdit",MUTED)
    var button := StyleBoxFlat.new(); button.bg_color=PANEL_2; button.border_color=Color("343b40"); button.set_border_width_all(1); button.set_corner_radius_all(6); button.content_margin_left=14; button.content_margin_right=14; button.content_margin_top=9; button.content_margin_bottom=9
    var button_hover := button.duplicate(); button_hover.bg_color=Color("2a3035"); button_hover.border_color=GOLD
    theme.set_stylebox("normal","Button",button); theme.set_stylebox("hover","Button",button_hover); theme.set_stylebox("pressed","Button",button_hover)
    var line := StyleBoxFlat.new(); line.bg_color=Color("101315"); line.border_color=Color("3b4247"); line.set_border_width_all(1); line.set_corner_radius_all(6); line.content_margin_left=12; line.content_margin_right=12; line.content_margin_top=10; line.content_margin_bottom=10
    theme.set_stylebox("normal","LineEdit",line)
    theme.set_stylebox("normal","TextEdit",line)
    self.theme=theme

func _panel_style(color:Color=PANEL) -> StyleBoxFlat:
    var s:=StyleBoxFlat.new(); s.bg_color=color; s.border_color=Color("30363a"); s.set_border_width_all(1); s.set_corner_radius_all(8); s.content_margin_left=16; s.content_margin_right=16; s.content_margin_top=14; s.content_margin_bottom=14; return s

func _build_ui() -> void:
    var bg:=ColorRect.new(); bg.color=BG; bg.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); add_child(bg)
    var root:=VBoxContainer.new(); root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); root.add_theme_constant_override("separation",8); root.offset_left=12; root.offset_right=-12; root.offset_top=10; root.offset_bottom=-10; add_child(root)
    root.add_child(_build_topbar())
    var body:=HBoxContainer.new(); body.size_flags_vertical=Control.SIZE_EXPAND_FILL; body.add_theme_constant_override("separation",8); root.add_child(body)
    var left_wrap:=PanelContainer.new(); left_wrap.custom_minimum_size.x=260; left_wrap.add_theme_stylebox_override("panel",_panel_style()); body.add_child(left_wrap)
    left_panel=VBoxContainer.new(); left_panel.add_theme_constant_override("separation",10); left_wrap.add_child(left_panel)
    var mid_wrap:=PanelContainer.new(); mid_wrap.add_theme_stylebox_override("panel",_panel_style(Color("15191c"))); mid_wrap.size_flags_horizontal=Control.SIZE_EXPAND_FILL; body.add_child(mid_wrap)
    center=VBoxContainer.new(); center.add_theme_constant_override("separation",12); mid_wrap.add_child(center)
    var right_wrap:=PanelContainer.new(); right_wrap.custom_minimum_size.x=290; right_wrap.add_theme_stylebox_override("panel",_panel_style()); body.add_child(right_wrap)
    right_panel=VBoxContainer.new(); right_panel.add_theme_constant_override("separation",10); right_wrap.add_child(right_panel)
    bottom_nav=HBoxContainer.new(); bottom_nav.alignment=BoxContainer.ALIGNMENT_CENTER; bottom_nav.add_theme_constant_override("separation",6); root.add_child(bottom_nav)
    for n in ["POSTAC","EKWIPUNEK","DZIENNIK","MAPA","KOMPENDIUM","KRONIKA","SWIAT","OBOZ"]:
        var b:=Button.new(); b.text=n; b.pressed.connect(_open_tab.bind(n)); bottom_nav.add_child(b)
    _make_overlay()

func _build_topbar() -> Control:
    var p:=PanelContainer.new(); p.add_theme_stylebox_override("panel",_panel_style(Color("171a1d")))
    var h:=HBoxContainer.new(); p.add_child(h)
    var brand:=Label.new(); brand.text="KRONIKI RPG"; brand.add_theme_font_size_override("font_size",23); brand.add_theme_color_override("font_color",GOLD); h.add_child(brand)
    var spacer:=Control.new(); spacer.size_flags_horizontal=Control.SIZE_EXPAND_FILL; h.add_child(spacer)
    status_line=Label.new(); status_line.add_theme_color_override("font_color",MUTED); h.add_child(status_line)
    var know:=Button.new(); know.text="TRYB WIEDZY"; know.pressed.connect(func(): knowledge_on=!knowledge_on; _render_right_context()); h.add_child(know)
    var ai:=Button.new(); ai.text="AI-MG"; ai.pressed.connect(_open_ai_settings); h.add_child(ai)
    var campaign:=Button.new(); campaign.text="NOWA KAMPANIA"; campaign.pressed.connect(_new_campaign); h.add_child(campaign)
    var load:=Button.new(); load.text="WCZYTAJ"; load.pressed.connect(_open_save_list); h.add_child(load)
    var save:=Button.new(); save.text="ZAPISZ"; save.pressed.connect(_open_save_dialog); h.add_child(save)
    return p

func _clear(node:Node) -> void:
    for c in node.get_children(): c.queue_free()

func _label(txt:String, size:int=17, color:Color=TEXT) -> Label:
    var l:=Label.new(); l.text=txt; l.autowrap_mode=TextServer.AUTOWRAP_WORD_SMART; l.add_theme_font_size_override("font_size",size); l.add_theme_color_override("font_color",color); return l

func _button(txt:String, callback:Callable) -> Button:
    var b:=Button.new(); b.text=txt; b.alignment=HORIZONTAL_ALIGNMENT_LEFT; b.pressed.connect(callback); return b

func _render_scene() -> void:
    current_tab="SCENA"; _clear(center); _render_left(); _render_right_context(); _update_status()
    center.add_child(_label("AKTYWNY WATEK",12,GOLD)); content_title=_label(state.world.quest,28,TEXT); center.add_child(content_title)
    var narrative:=RichTextLabel.new(); narrative.bbcode_enabled=false; narrative.fit_content=true; narrative.size_flags_vertical=Control.SIZE_EXPAND_FILL; narrative.custom_minimum_size.y=300
    narrative.text=last_narration
    center.add_child(narrative)
    var choices:=VBoxContainer.new(); choices.add_theme_constant_override("separation",6); center.add_child(choices)
    for a in suggested_actions:
        choices.add_child(_button(str(a),_do_action.bind(str(a))))
    if pending_request_id != "":
        choices.add_child(_button("ANULUJ ODPOWIEDŹ MG",_cancel_gm))
    action_input=LineEdit.new(); action_input.placeholder_text="Napisz wlasne dzialanie..."; action_input.text_submitted.connect(_submit_action); center.add_child(action_input)
    var h:=HBoxContainer.new(); center.add_child(h)
    for pair in [["ROZEJRZYJ SIE","Rozgladam sie uwaznie po okolicy."],["ROZMAWIAJ","Rozpoczynam rozmowe z najblizsza osoba."],["BADAJ","Badam otoczenie i szukam szczegolow."],["WALKA","__combat__"]]:
        h.add_child(_button(pair[0],_do_action.bind(pair[1])))

func _render_left() -> void:
    _clear(left_panel); var c=state.character
    left_panel.add_child(_label(c.name,25,GOLD)); left_panel.add_child(_label(c.profession+" • poziom "+str(c.level),14,MUTED))
    left_panel.add_child(_label("ZYCIE  "+str(c.hp)+" / "+str(c.max_hp),18,TEXT))
    left_panel.add_child(_label("Korony: "+str(c.gold),15,MUTED))
    left_panel.add_child(_label(_ai_label(),13,GREEN if ai_configured else MUTED))
    left_panel.add_child(HSeparator.new()); left_panel.add_child(_label("AKTUALNY CEL",12,GOLD)); left_panel.add_child(_label(state.world.quest,18,TEXT))
    left_panel.add_child(_label("Znajdz zaginionego zanim trop zostanie zmyty przez deszcz.",14,MUTED))
    left_panel.add_child(HSeparator.new()); left_panel.add_child(_label("STAN",12,GOLD)); left_panel.add_child(_label("Zmeczenie 22\nStres 2 / 10\nToksycznosc 0%\nRany 0",14,MUTED))

func _render_right_context() -> void:
    _clear(right_panel); right_panel.add_child(_label("KONTEKST",12,GOLD))
    if knowledge_on:
        right_panel.add_child(_label("TRYB WIEDZY",21,TEXT)); right_panel.add_child(_label("PERCEPCJA ★★★★☆",14,GOLD)); right_panel.add_child(_label("Mokry papier pachnie zelazem i dymem. Slad jest starszy niz dzisiejsza ulewa.",14,MUTED)); right_panel.add_child(_label("BESTIARIUSZ ★★★★☆",14,GOLD)); right_panel.add_child(_label("Brak wystarczajacych danych, by nazwac stworzenie. Nie zakladaj gatunku przed zebraniem kolejnych sladow.",14,MUTED)); return
    if state.combat.active:
        _render_combat_right(); return
    right_panel.add_child(_label("MARTA",22,TEXT)); right_panel.add_child(_label("karczmarka • ostrozna",14,MUTED)); right_panel.add_child(_meter("Zaufanie",42)); right_panel.add_child(_meter("Niepokoj",72)); right_panel.add_child(HSeparator.new()); right_panel.add_child(_label("SLEDZTWO",15,GOLD)); right_panel.add_child(_label("Dowody: "+str(state.clues.size())+"\nHipotezy: "+str(state.hypotheses.size())+"\nPewnosc identyfikacji: 31%",14,MUTED))

func _meter(name:String,value:int) -> VBoxContainer:
    var v:=VBoxContainer.new(); var l:=Label.new(); l.text=name+"  "+str(value)+"%"; v.add_child(l); var p:=ProgressBar.new(); p.value=value; p.show_percentage=false; v.add_child(p); return v

func _render_combat_right() -> void:
    var c=state.combat; right_panel.add_child(_label(c.enemy,24,RED)); right_panel.add_child(_label("Dystans %.1f m" % c.distance,14,MUTED)); right_panel.add_child(_meter("Garda",int(c.enemy_guard*16))); right_panel.add_child(_meter("Morale",c.morale)); right_panel.add_child(_label("Tempo: "+str(c.tempo)+"\nPresja: "+str(c.pressure)+"\nZamiar: obejscie bokiem",15,TEXT))
    for a in ["Natarcie","Finta","Obrona","Pozycja","Aard","Odwrót"]: right_panel.add_child(_button(a,_combat_action.bind(a)))

func _update_status() -> void:
    status_line.text="%s • %02d.%02d.%d • %02d:%02d • %s" % [state.world.location,state.world.day,state.world.month,state.world.year,state.world.hour,state.world.minute,state.world.weather]


func _ai_label() -> String:
    var mode := str(ai_info.get("mode","local"))
    var backend := str(ai_info.get("last_backend",""))
    if backend != "":
        return "MGAI: " + backend
    if mode == "local":
        return "MGAI: lokalny Qwen3-8B" if bool(ai_info.get("model_exists",false)) else "MGAI: model lokalny niepobrany"
    if mode == "hybrid":
        return "MGAI: hybrydowy"
    return "MGAI: online"

func _new_campaign() -> void:
    core.request("/campaign/new",{"character":state.get("character",{})})

func _submit_action(t:String) -> void:
    if t.strip_edges() != "":
        _do_action(t)

func _add_hypothesis(t:String) -> void:
    if t.strip_edges() != "":
        core.request("/hunt/action",{"kind":"hypothesis","label":t,"state":state})

func _do_action(a:String) -> void:
    if a=="__combat__": state.combat.active=true; state.world.mode="combat"; _render_scene(); return
    if pending_request_id != "": return
    pending_request_id="gm-"+str(Time.get_ticks_msec())
    core.request("/action",{"action":a,"state":state,"request_id":pending_request_id,"use_ai":true})
    if action_input: action_input.clear()
    _render_scene()

func _cancel_gm() -> void:
    if pending_request_id != "":
        core.request("/ai/cancel",{"request_id":pending_request_id})

func _combat_action(a:String) -> void:
    core.request("/combat/pulse",{"action":a,"combat":state.combat})

func _open_tab(name:String) -> void:
    current_tab=name; _clear(center); _render_left(); _render_right_context(); _update_status()
    match name:
        "POSTAC": _tab_character()
        "EKWIPUNEK": _tab_inventory()
        "DZIENNIK": _tab_journal()
        "MAPA": _tab_map()
        "KOMPENDIUM": _tab_compendium()
        "KRONIKA": _tab_chronicle()
        "SWIAT": _tab_world()
        "OBOZ": _tab_camp()
        _: _render_scene()

func _header(kicker:String,title:String) -> void:
    center.add_child(_label(kicker,12,GOLD)); center.add_child(_label(title,29,TEXT)); center.add_child(HSeparator.new())

func _tab_character() -> void:
    _header("BOHATER","Karta postaci")
    var c=state.character; center.add_child(_label(c.name+" — "+c.profession,23,GOLD))
    var grid:=GridContainer.new(); grid.columns=3; center.add_child(grid)
    for k in ["STR","DEX","CON","INT","PER","CHA"]: grid.add_child(_label(k+"  "+str(c.stats[k]),18,TEXT))
    center.add_child(_label("Umiejetnosci",18,GOLD)); center.add_child(_label(", ".join(c.skills),16,MUTED)); center.add_child(_button("NOWA POSTAC / KREATOR",_open_creator))

func _tab_inventory() -> void:
    _header("EKWIPUNEK","Sprzet i przygotowanie")
    for it in state.character.items: center.add_child(_button(it+"     [uzyj / wyposaz]",func(): pass))
    center.add_child(HSeparator.new()); center.add_child(_label("ALCHEMIA",17,GOLD)); center.add_child(_label("Jaskółka • Kot • Grom • Olej na nekrofagi",15,MUTED))
    center.add_child(_button("Uwarz Jaskółkę",func(): core.request("/alchemy/craft",{"recipe":"Jaskółka","state":state})))
    center.add_child(_button("Wytwórz zestaw naprawczy",func(): core.request("/crafting/craft",{"item":"Zestaw naprawczy","difficulty":2,"minutes":45,"state":state})))

func _tab_journal() -> void:
    _header("DZIENNIK","Zaginieni na trakcie")
    center.add_child(_label("DOWODY",16,GOLD));
    for x in state.clues: center.add_child(_label("✓ "+x,15,TEXT))
    center.add_child(_label("HIPOTEZY",16,GOLD));
    for x in state.hypotheses: center.add_child(_label("? "+str(x),15,MUTED))
    for h in state.get("hunting",{}).get("hypotheses",[]): center.add_child(_label("? "+str(h.get("label","hipoteza"))+" • "+str(h.get("confidence",0))+"%",15,MUTED))
    var add:=LineEdit.new(); add.placeholder_text="Dodaj wlasna hipoteze..."; add.text_submitted.connect(_add_hypothesis); center.add_child(add)

func _tab_map() -> void:
    _header("MAPA","Znane drogi")
    center.add_child(_label("NOVIGRAD\n     │ 18 km\n     ▼\nDOLNY BROD ───── 7 km ───── STARY MLYN\n     │ 12 km\n     ▼\nBAGNA",21,TEXT))
    center.add_child(_button("Wyrusz do Starego Mlyna • ok. 1 h 35 min",_travel.bind("Stary Mlyn",95)))
    center.add_child(_button("Wyrusz na Bagna • ok. 2 h 40 min",_travel.bind("Bagna",160)))

func _tab_compendium() -> void:
    _header("KOMPENDIUM","Wiedza bohatera")
    var q:=LineEdit.new(); q.placeholder_text="Szukaj: bruxa, Novigrad, Radovid..."; q.text_submitted.connect(func(t): core.request("/lore/search",{"q":t,"year":state.world.year,"month":state.world.month,"location":state.world.location})); center.add_child(q)
    var cats:=HBoxContainer.new(); center.add_child(cats)
    for n in ["Potwory","Postacie","Miejsca","Historia","Magia"]: cats.add_child(_button(n,func(): pass))
    lore_results=VBoxContainer.new(); center.add_child(lore_results); lore_results.add_child(_label("Wpisz haslo. Wyniki sa filtrowane przez date kampanii; interfejs gracza powinien ujawniac tylko poznana wiedze.",14,MUTED))

func _tab_chronicle() -> void:
    _header("ZYWA KRONIKA","Historia twojej kampanii")
    for e in state.chronicle:
        center.add_child(_label(e.date+"  •  "+e.title,17,GOLD)); center.add_child(_label(e.text,15,MUTED))

func _tab_world() -> void:
    _header("SWIAT","Historia i wydarzenia")
    for e in [[1263,"Upadek Cintry"],[1267,"Thanedd"],[1268,"Brenna i Rivia"],[1271,"Wydarzenia pierwszej gry"],[1272,"Wojna i Dziki Gon"],[1275,"Krew i Wino"]]:
        var mark:="PRZESZLOSC" if e[0]<state.world.year else ("TERAZ" if e[0]==state.world.year else "PRZYSZLOSC")
        center.add_child(_label(str(e[0])+"  ◆  "+e[1]+"   ["+mark+"]",17,TEXT if mark!="PRZYSZLOSC" else MUTED))
    center.add_child(_label("History Engine przechowuje osobno prawde swiata i wiedze bohatera. Przyszle wydarzenia nie sa ujawniane postaci.",14,MUTED))

func _tab_camp() -> void:
    _header("OBOZ","Co robisz przed snem?")
    for p in [["Napraw sprzet",60],["Opatrz rany",120],["Medytuj",60],["Studiuj notatki",90],["Rozmawiaj przy ogniu",45],["Spij",480]]:
        center.add_child(_button(p[0]+" • "+str(int(p[1]/60.0))+" h",_camp_action.bind(p[0],p[1])))

func _camp_action(name:String,mins:int) -> void:
    core.request("/world/tick",{"minutes":mins,"reason":name,"state":state})

func _travel(destination:String,mins:int) -> void:
    var next_state=state.duplicate(true); next_state.world.location=destination
    core.request("/world/tick",{"minutes":mins,"reason":"podroz","state":next_state})

func _advance_time(mins:int) -> void:
    var total:int = int(state.world.hour) * 60 + int(state.world.minute) + mins; state.world.day += int(total / 1440); total %= 1440; state.world.hour = int(total / 60); state.world.minute = total % 60; _update_status()

func _safe_character_name() -> String:
    var character = state.get("character", {})
    if character is Dictionary:
        var n := str(character.get("name", "Postać")).strip_edges()
        if n != "":
            return n
    return "Postać"

func _open_save_dialog() -> void:
    overlay.visible = true
    _clear(overlay)
    var v := VBoxContainer.new()
    v.add_theme_constant_override("separation", 10)
    overlay.add_child(v)
    v.add_child(_label("ZAPISZ GRĘ", 28, GOLD))
    v.add_child(_label("Utwórz nowy zapis albo nadpisz aktualnie wczytany slot.", 14, MUTED))
    var name := LineEdit.new()
    var world = state.get("world", {})
    var location := "Zapis"
    if world is Dictionary:
        location = str(world.get("location", "Zapis"))
    name.text = _safe_character_name() + " — " + location
    name.placeholder_text = "Nazwa zapisu"
    v.add_child(name)
    var h := HBoxContainer.new()
    v.add_child(h)
    h.add_child(_button("NOWY ZAPIS", func(): _save_game(name.text, false)))
    if save_id > 0:
        h.add_child(_button("NADPISZ BIEŻĄCY", func(): _save_game(name.text, true)))
    h.add_child(_button("ANULUJ", func(): overlay.visible = false))

func _save_game(name:String, overwrite:bool) -> void:
    var clean_name := name.strip_edges()
    if clean_name == "":
        clean_name = _safe_character_name() + " — zapis"
    save_pending_name = clean_name
    save_feedback = "Zapisywanie…"
    var p := {"name": clean_name, "state": state}
    if overwrite and save_id > 0:
        p["id"] = save_id
    core.request("/save", p)

func _combat_action_local(a:String) -> Dictionary:
    var c=state.combat.duplicate(true)
    match a:
        "Natarcie": c.enemy_guard=max(0,c.enemy_guard-1); c.pressure+=1
        "Finta": c.enemy_guard=max(0,c.enemy_guard-2); c.tempo=min(3,c.tempo+1)
        "Obrona": c.guard=min(8,c.guard+2)
        "Pozycja": c.tempo=min(3,c.tempo+1); c.distance=max(1.0,c.distance-0.5)
        "Aard": c.enemy_guard=max(0,c.enemy_guard-1); c.pressure+=2; c.distance+=1.5
        "Odwrót": c.distance+=2.0; c.tempo=max(-3,c.tempo-1)
    return c

func _on_core_response(route:String, ok:bool, data) -> void:
    if not ok:
        if route == "/save":
            save_feedback = "Nie udało się zapisać gry: " + str(data.get("error", "brak odpowiedzi Rust Core"))
            save_pending_name = ""
            overlay.visible = true
            _clear(overlay)
            var sv := VBoxContainer.new()
            sv.add_theme_constant_override("separation", 10)
            overlay.add_child(sv)
            sv.add_child(_label("BŁĄD ZAPISU", 28, RED))
            sv.add_child(_label(save_feedback, 15, TEXT))
            sv.add_child(_button("ZAMKNIJ", func(): overlay.visible = false))
            return
        if route=="/action": pending_request_id=""; last_narration="Rdzeń gry nie odpowiedział. Twoja akcja nie została utracona; możesz spróbować ponownie."
        if route=="/combat/pulse": state.combat=_combat_action_local("Obrona")
        _render_scene()
        return
    match route:
        "/ai/config", "/ai/status", "/ai/start-local":
            if data.get("status") is Dictionary:
                ai_info = data.get("status")
            else:
                ai_info = data
            if data.has("mode"): ai_info["mode"] = data.get("mode")
            ai_configured = bool(ai_info.get("model_exists",false)) or bool(ai_info.get("remote_configured",false))
            _render_left()
        "/lore/search":
            if lore_results:
                _clear(lore_results)
                for e in data.get("results",[]): lore_results.add_child(_label(str(e.get("title","Wpis"))+"\n"+str(e.get("summary","")),15,TEXT))
        "/combat/pulse":
            state.combat=data.get("combat",state.combat); _render_scene()
        "/action":
            pending_request_id=""
            if data.get("state") is Dictionary: state=data.get("state")
            last_narration=str(data.get("narration","Świat reaguje na twoje działanie."))
            suggested_actions=data.get("suggestions",suggested_actions)
            ai_info["last_backend"] = str(data.get("ai_backend",""))
            _render_scene()
        "/campaign/new":
            if data.get("state") is Dictionary:
                state=data.get("state")
                save_id=-1
                overlay.visible=false
                var ch=state.get("chronicle",[])
                if ch is Array and not ch.is_empty():
                    last_narration=str(ch[ch.size()-1].get("text","Nowa kampania rozpoczyna się."))
                suggested_actions=state.get("suggested_actions",suggested_actions)
                _render_scene()
        "/world/tick":
            if data.get("state") is Dictionary: state=data.get("state")
            _open_tab(current_tab)
        "/hunt/action", "/alchemy/craft", "/crafting/craft", "/magic/cast", "/npc/event", "/quest/update":
            if data.get("state") is Dictionary: state=data.get("state")
            _open_tab(current_tab)
        "/save":
            if bool(data.get("ok", false)):
                save_id = int(data.get("id", save_id))
                save_feedback = "Zapisano: " + (save_pending_name if save_pending_name != "" else "gra")
                save_pending_name = ""
                overlay.visible = true
                _clear(overlay)
                var v := VBoxContainer.new()
                v.add_theme_constant_override("separation", 10)
                overlay.add_child(v)
                v.add_child(_label("GRA ZAPISANA", 28, GREEN))
                v.add_child(_label(save_feedback, 16, TEXT))
                v.add_child(_button("WRÓĆ DO GRY", func(): overlay.visible = false))
            else:
                save_feedback = "Nie udało się zapisać gry: " + str(data.get("error", "nieznany błąd"))
                save_pending_name = ""
                overlay.visible = true
                _clear(overlay)
                var v := VBoxContainer.new()
                v.add_theme_constant_override("separation", 10)
                overlay.add_child(v)
                v.add_child(_label("BŁĄD ZAPISU", 28, RED))
                v.add_child(_label(save_feedback, 15, TEXT))
                v.add_child(_button("ZAMKNIJ", func(): overlay.visible = false))
        "/saves":
            _show_save_list(data.get("results",[]))
        "/save/load":
            if data.get("state") is Dictionary:
                state=data.get("state"); save_id=int(data.get("id",-1)); overlay.visible=false; _render_scene()
        "/ai/cancel":
            if bool(data.get("ok",false)): pending_request_id=""; last_narration="Odpowiedź MG została anulowana. Stan mechaniczny pozostaje bezpieczny."; _render_scene()

func _make_overlay() -> void:
    overlay=PanelContainer.new(); overlay.visible=false; overlay.set_anchors_and_offsets_preset(Control.PRESET_CENTER); overlay.custom_minimum_size=Vector2(760,650); overlay.add_theme_stylebox_override("panel",_panel_style(Color("141719"))); add_child(overlay)

func _open_ai_settings() -> void:
    overlay.visible=true; _clear(overlay)
    var v:=VBoxContainer.new(); v.add_theme_constant_override("separation",10); overlay.add_child(v)
    v.add_child(_label("MISTRZ GRY AI",28,GOLD))
    v.add_child(_label("Standard dla tego komputera: Qwen3-8B Q5_K_M • okno kontekstu 12k • maksymalny offload GPU. Rust nadal rozstrzyga mechanikę i waliduje stan.",14,MUTED))
    var mode:=OptionButton.new()
    mode.add_item("Lokalny",0); mode.add_item("Hybrydowy",1); mode.add_item("Online",2)
    var current_mode:=str(ai_info.get("mode","local"))
    mode.select(1 if current_mode=="hybrid" else (2 if current_mode=="online" else 0))
    v.add_child(mode)
    var installed:=bool(ai_info.get("model_exists",false))
    var server_ok:=bool(ai_info.get("server_exists",false))
    v.add_child(_label("Model: "+("znaleziony" if installed else "brak pliku")+" • llama.cpp: "+("gotowy" if server_ok else "brak")+"\n"+str(ai_info.get("model_path","%LOCALAPPDATA%/KronikiRPG/models/Qwen3-8B-Q5_K_M.gguf")),14,GREEN if installed and server_ok else MUTED))
    var key:=LineEdit.new(); key.placeholder_text="Opcjonalny klucz API dla trybu Hybrydowego/Online"; key.secret=true; v.add_child(key)
    var model:=LineEdit.new(); model.text=str(ai_info.get("remote_model","gpt-5.6-luna")); model.placeholder_text="Model online"; v.add_child(model)
    var h:=HBoxContainer.new(); v.add_child(h)
    h.add_child(_button("ZAPISZ",func():
        var selected_mode="local" if mode.selected==0 else ("hybrid" if mode.selected==1 else "online")
        core.request("/ai/config",{"mode":selected_mode,"api_key":key.text,"model":model.text,"local_context":12288,"local_gpu_layers":99,"timeout_ms":120000,"retries":1})
        overlay.visible=false
    ))
    h.add_child(_button("URUCHOM LOKALNY MGAI",func(): core.request("/ai/start-local",{})))
    h.add_child(_button("ANULUJ",func(): overlay.visible=false))

func _open_save_list() -> void:
    core.request("/saves",{},"GET")

func _show_save_list(items:Array) -> void:
    overlay.visible=true; _clear(overlay)
    var v:=VBoxContainer.new(); v.add_theme_constant_override("separation",8); overlay.add_child(v); v.add_child(_label("WCZYTAJ GRĘ",28,GOLD))
    if items.is_empty(): v.add_child(_label("Brak zapisów.",15,MUTED))
    for it in items:
        v.add_child(_button(str(it.get("name","Zapis"))+" • "+str(it.get("updated_at","")),_load_save.bind(int(it.get("id",-1)))))
    v.add_child(_button("ZAMKNIJ",func(): overlay.visible=false))

func _load_save(id:int) -> void:
    core.request("/save/load",{"id":id})

func _open_creator() -> void:
    overlay.visible=true; _clear(overlay)
    var v:=VBoxContainer.new(); v.add_theme_constant_override("separation",10); overlay.add_child(v); v.add_child(_label("KREATOR POSTACI",28,GOLD)); v.add_child(_label("Wybierz droge. Pochodzenie zmienia wiedze, startowe zasoby i pierwszy rozdzial kampanii.",14,MUTED))
    var name:=LineEdit.new(); name.placeholder_text="Imie"; name.text=state.character.name; v.add_child(name)
    var profession:=OptionButton.new()
    for x in ["Wiedzmin","Czarodziej","Lowca potworow","Zolnierz","Medyk","Uczony","Przemytnik"]:
        profession.add_item(x)
    v.add_child(profession)
    var origin:=OptionButton.new()
    for x in ["Czlowiek szlaku","Ocalaly z wojny","Wygnaniec z dworu","Polswiatek","Uczen mistrza","Wlasna droga"]:
        origin.add_item(x)
    v.add_child(origin)
    v.add_child(_label("STATYSTYKI — 8 punktow ponad baze",15,GOLD)); var info:=_label("STR 3 • DEX 5 • CON 4 • INT 3 • PER 5 • CHA 2",16,TEXT); v.add_child(info)
    var concept:=TextEdit.new(); concept.custom_minimum_size.y=120; concept.placeholder_text="Opis postaci / motywacja / tajemnica..."; v.add_child(concept)
    var h:=HBoxContainer.new(); v.add_child(h); h.add_child(_button("GENERUJ PROPOZYCJE AI",func(): core.request("/character/generate",{"concept":concept.text,"profession":profession.get_item_text(profession.selected),"year":state.world.year})))
    h.add_child(_button("ROZPOCZNIJ KAMPANIE",func():
        state.character.name=name.text if name.text.strip_edges()!="" else "Bez imienia"
        state.character.profession=profession.get_item_text(profession.selected)
        state.character["origin_story"]=origin.get_item_text(origin.selected)
        state.character["concept"]=concept.text
        core.request("/campaign/new",{"character":state.character})
    ))
    h.add_child(_button("ANULUJ",func(): overlay.visible=false))

