# Kroniki RPG 0.10 — Autonomous MG Foundation

Ten zestaw źródeł przenosi projekt z prototypu „narrator + UI” w stronę autonomicznego Mistrza Gry.

## Zaimplementowane w Rust Core

- natural-language intent interpreter,
- mechaniczne rozstrzygnięcie przed narracją,
- free-form magic semantics,
- AI context builder,
- lokalny model przez OpenAI-compatible endpoint (np. llama.cpp server),
- timeout + lokalny fallback,
- walidacja AI state patches,
- NPC memory i relacje NPC↔NPC / NPC↔player,
- frakcje i ich clocks,
- strukturalne questy i deadline'y,
- background world simulation,
- Pulsy Starcia state,
- hunting evidence/confidence,
- alchemy,
- crafting,
- trwały SQLite save/load,
- AI config w SQLite,
- event log kampanii.

## Zaimplementowane w Godot

- główny ekran Living Chronicle,
- free-form input,
- sugestie MGAI,
- anulowanie requestu AI,
- komunikat fallbacku,
- save/load,
- status Core/AI,
- podstawowy kontekst postaci/sceny/walki.

## Lokalny model

Profil domyślny dla maszyny referencyjnej 16 GB RAM / 8 GB VRAM:

- Qwen3-8B,
- GGUF Q5_K_M,
- llama.cpp,
- ok. 14k kontekstu,
- 14B Q4_K_M jako tryb High/Experimental.

Sam model GGUF nie jest commitowany do repozytorium. Jest dużym opcjonalnym pakietem i docelowo pobierze go launcher.

## Kluczowa reguła

AI nie jest autorytetem stanu. Model proponuje narrację i patch, a Rust Core waliduje i zatwierdza mechanikę, wiedzę oraz konsekwencje.
