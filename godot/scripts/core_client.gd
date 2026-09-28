class_name CoreClient
extends Node

signal core_ready(info)
signal request_completed(route, ok, data)
signal request_cancelled(request_id)

var base_url := "http://127.0.0.1:17377"
var core_pid := -1
var _seq := 0
var _requests := {}
var timeout_seconds := 18.0

func _ready() -> void:
    _ensure_core()

func _ensure_core() -> void:
    var exe := _core_executable()
    if FileAccess.file_exists(exe):
        core_pid = OS.create_process(exe, PackedStringArray())
    await get_tree().create_timer(0.45).timeout
    request("/health", {}, "GET")

func _core_executable() -> String:
    var name := "kroniki_core.exe" if OS.get_name() == "Windows" else "kroniki_core"
    var beside := OS.get_executable_path().get_base_dir().path_join(name)
    if FileAccess.file_exists(beside):
        return beside
    var dev := ProjectSettings.globalize_path("res://../rust-core/target/release/" + name)
    if FileAccess.file_exists(dev):
        return dev
    return beside

func request(route:String, payload:Dictionary = {}, method:String = "POST") -> int:
    _seq += 1
    var id := _seq
    var h := HTTPRequest.new()
    h.timeout = timeout_seconds
    add_child(h)
    _requests[h] = {"route": route, "id": id}
    h.request_completed.connect(_on_completed.bind(h))
    var headers := PackedStringArray(["Content-Type: application/json"])
    var body := JSON.stringify(payload)
    var m := HTTPClient.METHOD_GET if method == "GET" else HTTPClient.METHOD_POST
    var err := h.request(base_url + route, headers, m, "" if method == "GET" else body)
    if err != OK:
        _requests.erase(h)
        h.queue_free()
        request_completed.emit(route, false, {"error": "Nie można wysłać żądania do Rust Core.", "request_id": id})
    return id

func cancel(request_id:int) -> void:
    for h in _requests.keys():
        var meta:Dictionary = _requests[h]
        if int(meta.get("id", -1)) == request_id:
            h.cancel_request()
            _requests.erase(h)
            h.queue_free()
            request_cancelled.emit(request_id)
            return

func _on_completed(_result:int, code:int, _headers:PackedStringArray, body:PackedByteArray, h:HTTPRequest) -> void:
    var meta:Dictionary = _requests.get(h, {})
    _requests.erase(h)
    var route:String = str(meta.get("route", ""))
    var id:int = int(meta.get("id", -1))
    var txt := body.get_string_from_utf8()
    var data = JSON.parse_string(txt)
    if data == null:
        data = {"raw": txt}
    if data is Dictionary:
        data["request_id"] = id
    var ok := code >= 200 and code < 300 and bool(data.get("ok", true))
    request_completed.emit(route, ok, data)
    if route == "/health" and ok:
        core_ready.emit(data)
    h.queue_free()

func _exit_tree() -> void:
    if core_pid > 0:
        request("/shutdown", {})
        core_pid = -1
