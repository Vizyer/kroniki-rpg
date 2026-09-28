# Aktualizacje z GitHub

Repozytorium jest źródłem kodu. Edytuj `rust-core/`, `godot/`, `launcher/`, `installer/` i `scripts/` w katalogu głównym. Fragmenty w `ci/` i historyczne `source010/current` nie są wejściem nowego builda. CI nie wykonuje materializacji, commitów ani force-pushów do gałęzi rozwojowych.

## Publikacja

1. Zmiany trafiają do PR do `main`. Windows sprawdza Core i launcher, tworzy instalator oraz sprawdza zapis po restarcie, podmianie aplikacji i powrocie do poprzedniej paczki.
2. Po scaleniu do `main` ten sam workflow tworzy osobną wersję `X.Y.Z-preview.RUN.ATTEMPT`, gdzie `X.Y.Z` pochodzi z `rust-core/Cargo.toml`.
3. Dopiero po testach osobne zadanie z `contents: write` przesyła instalator, ZIP i manifest do szkicu GitHub Release. Ujawnia wydanie po przesłaniu wszystkich plików. Nie nadpisuje poprzednich tagów ani zasobów.
4. Pierwsza instalacja i aktualizacje samego launchera odbywają się przez `KronikiRPG-Setup.exe` z Releases. Zainstalowany launcher pobiera aktualizacje gry z tego samego repozytorium, z kanału w `launcher-config.json` (obecnie `preview`).

Launcher sprawdza wersję semver, kanał, tag i adres paczki, SHA-256, rozmiar (jeżeli podany) oraz zgodność `version.json` z manifestem. Dopiero po walidacji odkłada poprzedni katalog `app` do `.rollback` i aktywuje nowy. Przy błędzie aktywacji próbuje przywrócić poprzedni katalog. Zapisy i model AI pozostają poza katalogiem `app`, w LocalAppData. Paczka nie aktualizuje uruchomionego launchera; wymaganie nowszego launchera oznacza konieczność pobrania instalatora.

Manifest i paczka zawierają `source_sha`, wskazujące dokładny commit. Wydania próbne nie są oznaczane jako stabilne. Stabilne wydanie wymaga oddzielnej, świadomej decyzji o wersji i kanale.

Historyczna gałąź `feature/0.10-autonomous-mg` zachowuje swoją historię; nie należy scalać jej snapshotu na ślepo. Odzyskiwanie poszczególnych funkcji z niej powinno odbywać się przez PR z testami API.
