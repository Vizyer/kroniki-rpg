# Kroniki RPG — prywatna kampania z autonomicznym MG

Status: plan rozwoju, nie lista gotowych funkcji. Punkt odniesienia: Friends & Fables. Stan bazowy: main `4aa2100e7a40792e5654df116aef55ce2da04627`, preview 0.10.0-preview.33.1. Opracowano 29.09.2026.

## Cel użytkownika

Aplikacja do prywatnych sesji RPG, w której gracz prowadzi swoją postać, a MG sam przygotowuje i prowadzi przygodę, odgrywa postacie oraz pilnuje konsekwencji. GitHub pozostaje źródłem kodu i aktualizacji. Zapisy prywatnych kampanii pozostają poza repozytorium.

Fables jest punktem odniesienia dla kompletności doświadczenia: kampania, narracja, postać, świat, NPC, mapa, ekwipunek i dziennik dostępne razem. Zachowujemy obecny klient Godot i Rust Core; odniesienie do strony internetowej samo w sobie nie oznacza migracji projektu do przeglądarki. Interfejs, scenariusze i grafika Kronik będą własne.

## Co potwierdzono o Fables

Publiczna strona opisuje AI MG, narzędzia budowy świata, podróże, taktyczną walkę inspirowaną 5e, multiplayer, mapy oraz głos. Opis Franz 2.0 dokumentuje dobór lore i wspomnień przed odpowiedzią, aktualizowany plan fabuły oraz kontekst sceny. Sama nazwa „autonomiczny MG” nie będzie zatem wyróżnikiem Kronik.

Nie przeprowadzono sesji porównawczej w zalogowanej aplikacji: play.fables.gg przekierowuje do logowania. Opisy producenta potwierdzają deklarowane funkcje, nie ich jakość. Notatka Franz 2.0 mówi o wycofaniu generowania formalnych questów w locie na rzecz prowadzenia narracyjnego; starszych przełączników generowania nie traktujemy jako dowodu obecnego działania.

Źródła:
- https://fables.gg/
- https://fables.gg/patch-notes/franz-20-working-context-lore-improved-planning-and-more
- https://fables.gg/patch-notes/ace-15-bringing-pois-and-npcs-to-life-npc-conversations

## Co znaczy „w miarę autonomiczny”

Domyślny tryb: aktywny MG. Rozwija sytuację w czasie gry i zatrzymuje się w miejscu wymagającym decyzji gracza. Potrafi rozpocząć rozmowę NPC, wprowadzić konsekwencję lub zaproponować okazję bez polecenia „wymyśl mi zadanie”.

- MG wybiera reakcje świata, cele NPC, dostępne sceny, stawki prób i możliwe konsekwencje.
- Gracz wybiera działania, wypowiedzi, przekonania i decyzje własnej postaci.
- Upływ czasu w świecie wynika z działań, podróży, odpoczynku lub jawnego polecenia kontynuacji. Zamknięcie aplikacji domyślnie zatrzymuje kampanię.
- Samodzielna sekwencja MG ma limit zdarzeń i wywołań modelu; nie prowadzi nieskończonej rozmowy sam ze sobą.
- Zmiany świata zatwierdza silnik. Narracja nie stanowi polecenia zmiany zapisu.
- Kontrola intensywności: spokojny MG, aktywny MG, symulacja świata. To planowane ustawienia, nie obecne opcje UI.

Przykład odbioru: gracz ignoruje prośbę kupca i spędza dzień w innym miejscu. Kupiec szuka pomocy u kogoś innego, przeciwnik realizuje etap planu, a po powrocie gracz zastaje wynik tych działań. Każda zmiana ma przyczynę, czas i uczestników. MG nie dopisuje, że bohater przyjął zadanie.

## Obecny fundament i braki

| Obszar | Jest w 0.10 preview.33.1 | Docelowa rozbudowa |
|---|---|---|
| Prowadzenie kampanii | Jeden ustalony scenariusz, wątki i wskazówki | Przygotowanie kampanii z założeń, rozgałęzienia i zmiana planu po decyzjach |
| NPC | Osobowość, proste reakcje, relacje i pamięć | Cele, zasoby, wiedza, harmonogramy i plan działań z warunkami |
| Pamięć | Ostatnie tury, starsze skróty, dobór słów | Trwałe obietnice i fakty ze źródłem, dobór według osób i wątków |
| Świat | Znane lokacje, czas, zalążek symulacji | Zdarzenia wynikające z upływu czasu i zależności między frakcjami |
| Mechanika | Podstawowe próby, koszty i struktury systemów | Spójne reguły wybranego systemu, pełna walka i rozwój postaci |
| Interfejs | Narracja, podstawowe panele i dziennik | Karta, ekwipunek, relacje, mapa i czytelna historia sesji |
| Prywatna sesja | Lokalny zapis i model, autosave | Wygodny eksport i kopie; później prywatna drużyna sieciowa |

## Kolejność wdrażania

### A. Autonomiczne cele NPC i konsekwencje czasu — najbliższy etap

Pliki: `rust-core/src/dm.rs`, `domain.rs`, `engine.rs`, `systems.rs`, `store.rs`, `ai.rs` oraz `rust-core/tests/campaign_dm.rs`.

Dodać typowane cele i zaplanowane zdarzenia: identyfikator, właściciel, warunki, czas gry, koszt, efekt i widoczność. Zastąpić ogólne zwiększanie pilności co kilka tur konsekwencjami powiązanymi z rzeczywistym czasem i sytuacją. Silnik sprawdza dostępność NPC, zasoby i możliwość wykonania. Prywatny planista może korzystać z sekretów świata; narrator otrzymuje wyłącznie zatwierdzone zdarzenia widoczne graczowi. Informacja o prywatnym celu nie może pojawić się przez log, dziennik, publiczne API ani kontekst narratora.

Kryteria odbioru:
1. Dwa różne wybory gracza prowadzą do odmiennych, zapisanych konsekwencji.
2. NPC nie wykonuje działania bez wymaganych zasobów lub po utracie możliwości.
3. Odpoczynek i podróż uruchamiają należne zdarzenia dokładnie raz, także po restarcie.
4. Pauza aplikacji nie przesuwa zegara kampanii.
5. Anulowanie tury nie pozostawia połowy zmian; ponowienie żądania nie dubluje zdarzenia.
6. NPC zna tylko fakty, których był świadkiem albo które mu przekazano.

### B. Pamięć faktów i zobowiązań

Fakt ma źródło, uczestników, czas, poziom pewności i zakres wiedzy. Oddzielić wydarzenie od plotki i deklaracji gracza. Zapisać obietnice, długi, konflikty i rozstrzygnięte wątki. Dobierać kontekst według aktualnego problemu, z limitem dla lokalnego modelu. Cofnięcie zapisu przywraca także zakres wiedzy; przyszłe wydarzenia nie wracają z archiwum.

Odbiór: po 100 turach i restarcie MG przypomina istotną obietnicę; nie przypisuje wiedzy nieobecnej postaci; korekta jednego faktu nie nadpisuje całej historii.

### C. Kampanie tworzone z założeń gracza

Kreator: świat, bohater, ton, długość przygody, granice treści i intensywność MG. Generator tworzy mały, spójny obszar, postacie, konflikty i kilka dróg działania. Walidacja odrzuca duplikaty, zerwane odwołania i niedostępne rozwiązania. Scenariusz startowy pozostaje powtarzalnym scenariuszem testowym.

Odbiór: można rozpocząć dwie rzeczywiście różne przygody; odrzucenie głównego tropu nie blokuje kampanii; nowe elementy mają trwałe identyfikatory i nie zmieniają się po ponownym uruchomieniu.

### D. Pełny ekran sesji i mechanika

Pliki UI: `godot/scripts/main.gd`, `core_client.gd`, a następnie wydzielone sceny i komponenty. Centrum: narracja i działanie. Panele: bohater/ekwipunek oraz scena/NPC. Zakładki: mapa, dziennik, relacje i zasady. Wyraźnie pokazywać próbę, koszt i zatwierdzony skutek. Dobór zasad walki wymaga ustalenia docelowego systemu; inspiracja Fables nie narzuca D&D 5e.

Odbiór: gra nie wymaga edycji JSON ani ręcznego poprawiania HP i ekwipunku. Narracja, karta i mapa pokazują ten sam stan.

### E. Prywatna drużyna i dodatki

Dopiero po stabilnej sesji: role gospodarza i graczy, indywidualna wiedza, kolejność działań, rozłączanie i powrót do sesji. Głos, ilustracje i rozbudowane mapy jako dodatki. Przy lokalnym modelu brak limitu tur narzuconego przez dostawcę usługi; wydajność nadal ograniczają komputer i budżet kontekstu. Zewnętrzne modele mogą mieć własne koszty i limity.

## Jak sprawdzimy, czy jest lepiej

Przewaga nad Fables jest celem, nie potwierdzonym wynikiem. Najpierw oceniamy Kroniki względem własnej wersji bazowej: spójność 100 tur, liczba koniecznych korekt gracza, odtwarzanie pamięci, faktyczne konsekwencje wyborów, zachowanie sekretów, czas do pierwszej odpowiedzi i częstość trybu awaryjnego. Zestaw stałych scenariuszy plus prawdziwe sesje z Qwen na docelowym komputerze. Porównanie obu produktów wymaga tych samych scenariuszy i zanotowanych wyników, nie porównania opisów marketingowych.

Testy mechaniki i komunikacji HTTP nie potwierdzają jakości improwizacji. Każdy etap powinien kończyć się krótką grywalną sesją, testami trwałości i dopiero potem nową aktualizacją preview.
