# MG kampanii 0.10

W aplikacji wybierz **NOWA KAMPANIA**, wpisz imię, opcjonalny ton sesji i tematy do unikania. Rozpoczyna się „Dzwon nad brodem”: gospoda, stary most, młyn, trzech NPC i zagadka zaginionego kuriera. Tytuł zmienia nazwę zapisu, nie generuje innego scenariusza. Model lokalny pobiera się w launcherze; bez niego działa prosty narrator awaryjny.

**DZIENNIK** pokazuje wątki, odkryte wskazówki i ostatnie 12 tur. Każda zatwierdzona tura jest automatycznie zapisywana w SQLite, razem z pamięcią kampanii. Po ponownym uruchomieniu wraca ostatnia aktywna kampania. ZAPISZ tworzy lub aktualizuje ręczny punkt powrotu; przed nową kampanią zachowaj taki zapis. Wczytanie wcześniejszego zapisu przywraca również ówczesną pamięć, bez wiedzy o późniejszych turach.

## Jak MG prowadzi turę

1. Model interpretuje intencję i proponuje maksymalnie dwie reakcje obecnych NPC oraz otwarty wątek.
2. Silnik sprawdza identyfikatory, rozstrzyga próbę, koszty i czas, ujawnia dostępne wskazówki oraz aktualizuje relacje i wątki.
3. Narrator otrzymuje zatwierdzone zdarzenia, publiczny stan i pamięć. Ukryte wskazówki, prywatne plany, sekrety NPC i surowa prawda świata nie trafiają do tego kontekstu.
4. Stan i zapis tury trafiają do jednej transakcji SQLite. Błąd zapisu nie zmienia stanu w pamięci.

Plan modelu nie może zawierać dowolnego patcha, HP, ekwipunku ani nowych faktów. Obsługiwane reakcje NPC to pytanie, deklaracja pomocy, odmowa i ostrzeżenie. Podróż zmienia lokację wyłącznie na znaną sąsiednią. Sukces badania może ujawnić wskazówkę dopiero po spełnieniu jej warunków. Nacisk na wątki rośnie co sześć tur. To podstawowy reżyser scen, nie pełna symulacja społeczeństwa.

Pamięć zawiera 12 ostatnich tur, do 256 starszych skrótów i do 64 zbiorczych podsumowań. Kontekst dobiera starsze wspomnienia przez podobieństwo słów. Pełne zdarzenia tur pozostają w lokalnej tabeli audytu `events`; nie są automatycznie wyszukiwane po wczytaniu wcześniejszego zapisu. Kontekst modelu ma konserwatywny limit znaków, a starsze wspomnienia są pomijane, kiedy nie mieszczą się w budżecie.

## Oczekiwanie i anulowanie

Model może potrzebować do 60 sekund na załadowanie. Plan ma limit 20 sekund, narracja 65 sekund, cała tura 150 sekund; klient czeka dłużej niż serwer. Błędy modelu uruchamiają narrację awaryjną. ANULUJ przekazuje identyfikator tury do serwera i przerywa oczekiwanie. Jeśli zapis wygrał wyścig z anulowaniem, aplikacja pokazuje już zatwierdzony stan; nie obiecuje cofnięcia zakończonej tury.

## Testowanie

- `cd rust-core && cargo test`: istniejące testy i scenariusze `tests/campaign_dm.rs`: odkrywanie zagadki, granice wiedzy, walidacja planów, relacje, czas, pamięć 350 tur, cofanie zapisu, restart, błędy SQLite, dwufazowy model HTTP i anulowanie podczas generowania.
- CI sprawdza Rust, launcher i parsowanie Godot.
- Windows preview uruchamia instalator, turę kampanii, zapis, restart, aktualizację i powrót do poprzedniego pakietu, kontrolując zachowanie pamięci.

## Granice tej wersji

To działający fundament narratywnego MG, nie zamiennik doświadczonego człowieka we wszystkich systemach RPG. Scenariusz startowy jest autorsko ustalony. Brakuje generatora długich kampanii, pełnej walki taktycznej, rozbudowanych autonomicznych planów NPC, semantycznego wyszukiwania całej historii i przetestowanej wieloosobowej sesji sieciowej. Ton i granice są instrukcjami dla modelu, nie gwarantowanym filtrem treści. Proza modelu może zawierać niespójności, mimo że nie może zmieniać autorytatywnego stanu. Testy HTTP używają kontrolowanego modelu zastępczego; jakość dialogów i szybkość Qwen wymagają sesji na docelowym sprzęcie.

## Pełna przygoda „Dzwon nad brodem” — nowy przebieg

Dotyczy nowych kampanii. Stare zapisy zachowują dotychczasowy scenariusz; nie dodajemy do trwającej sesji nowych terminów bez wiedzy gracza.

Przygoda ma ustaloną prawdę: Jan dostarczył lekarstwo rodzinie Eli, został ranny i ukrywa się w młynie. Poborca szuka go za zaległe myto. Marta może przekazać list, Bor wskazuje drogę, Ela może pomóc w ucieczce. Można pominąć śledztwo na moście i bezpośrednio poprosić Elę o pomoc.

Czas jest czasem gry: rozmowa lub prosta obserwacja trwa co najmniej minutę, podróż 10 minut, odpoczynek 30 minut. Po 30 minutach pojawia się ostrzeżenie, po 60 poborca przyjmuje świadków na moście, po 90 kurier zostaje zatrzymany, po 120 eskorta odjeżdża. Ogłoszenia woźnego są jawne; przyszłe zdarzenia nie trafiają do kontekstu narratora. Zamknięcie aplikacji nie przesuwa czasu. W tej małej przygodzie działania NPC są napisanym scenariuszem z warunkami, nie jeszcze ogólnym generatorem planów.

Ważne decyzje wybiera się z propozycji pod narracją lub wpisuje ich dokładny tekst. Model nadal obsługuje swobodne rozmowy, ale nie może sam zatwierdzić wydania kuriera czy zakończenia sprawy. Dostępne drogi:

- rozmowa z Elą i pomoc Janowi w ucieczce;
- list Marty oraz poręczenie u poborcy na moście;
- świadome wydanie Jana;
- próba odbicia po zatrzymaniu (rozstrzygnięcie mechaniczne, możliwa porażka);
- opuszczenie okolicy;
- utrata okazji do interwencji po odjeździe eskorty.

Poznanie miejsca ukrycia samo w sobie nie kończy zadania. Zakończenie ma trwały epilog, stan kuriera oraz konsekwencje w relacjach świadków. Dalsza gra po epilogu wymaga nowej kampanii lub wczytania wcześniejszego zapisu. To zamknięta przygoda testowa, a nie otwarta, nieskończona kampania.

### Próba z lokalnym modelem na komputerze gracza

1. Pobierz model w launcherze i rozpocznij nową kampanię. Sprawdź, czy status po odpowiedzi wskazuje model, a nie tryb awaryjny.
2. Porozmawiaj swobodnie z Martą przez kilka tur. Zapytaj, skąd zna Jana. Sprawdź, czy nie zmienia jego imienia, motywu poborcy lub losu lekarstwa.
3. Wybierz prośbę o pomoc, następnie idź do młyna, poproś Elę o odnalezienie kuriera i pomóż w ucieczce. Przed decyzją zrób zapis.
4. Zamknij i uruchom grę ponownie. Epilog oraz dziennik powinny pozostać takie same.
5. Wczytaj zapis sprzed decyzji i wybierz wydanie kuriera. Sprawdź odmienne zakończenie i brak wspomnień z poprzedniej, cofniętej ścieżki.
6. W nowej sesji poczekaj na poborcę, przedstaw list i poręcz za Jana. W kolejnej zignoruj sprawę do odjazdu eskorty.

Testy automatyczne w `tests/bell_adventure.rs` sprawdzają gałęzie, upływ czasu, porażkę, zapis, stare schematy i granice wiedzy. Nie zastępują oceny dialogu rzeczywistego Qwen. Nie deklarujemy jej ukończenia na podstawie testów silnika.
