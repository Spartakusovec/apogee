# Návrh: aplikace Settings

Jedna aplikace, ve které jde nastavit všechno, co se dnes dělá příkazy v terminálu nebo
přes Clauda. Texty v aplikaci anglicky (jako celé prostředí), kód a komentáře česky.
Stav (18. 11. 2026): hotové stránky Wi-Fi (sbalení po 8, podrobnosti sítě s IP/DNS/heslem,
eduroam a podnikové sítě), Bluetooth, Sound, Display, Power & Battery, Appearance, About
(kód v `src/nastaveni/`). Nastavení Swaye ukládá `nastaveni.conf` (include na konci configu).
Pak (18. 11.) Keyboard & Touchpad (rozložení + ukazatel v panelu, touchpad, myš), Panel & Dock
(moduly, hodiny, schovávání doku, připnuté), Windows & Workspaces (výchozí dlaždice, mezery),
Shortcuts, Notifications (Nerušit podle času, ztlumení aplikací), Apps (výchozí, po přihlášení),
Date & Time (pásmo, NTP, formáty), Network (letadlo, kabel, VPN import, hotspot), Users (obrázek,
jméno, heslo přes pkexec chpasswd), Storage (du, úklid), Updates (dnf/Flatpak, sway-patch, fwupd).
Postranní panel ve skupinách Connections / Device / Personalization / System. Vzor rozsahu: COSMIC Settings.

## Jak se chová

- **Část pomocníka** (Rust, GTK 4, stejný démon a stejné barvy z tapety): `apogee nastaveni [stránka]`.
  Otevře se hned (okno se sestaví při prvním otevření, stránky až když na ně přijde řada).
- **Normální okno**, ne vyskakovací okénko: dá se přesunout, maximalizovat, minimalizovat,
  má vlastní lištu s tlačítky. Žádné průhlednosti, jen barvy tématu.
- **Odkud se otevírá**: z nabídky vypnutí (položka Settings), ze Spotlightu (napsat „wifi“,
  „bluetooth“, „sound“… otevře rovnou správnou stránku), a každé okénko v panelu (Wi-Fi,
  Bluetooth, zvuk, baterie…) má dole odkaz „Settings…“ na svou stránku. Rychlá okénka v panelu
  zůstávají na rychlé věci, aplikace je na všechno ostatní.
- **Změny platí hned**, bez tlačítka Použít. Výjimka: změna rozlišení / měřítka obrazovky se
  15 s ptá „Keep these settings?“ a jinak se vrátí (jako GNOME).
- **Nikdy terminál**: co potřebuje práva roota (conservation mode baterie, aktualizace,
  parametry jádra), se zeptá na heslo běžným dialogem (polkit) přes malý pomocný program.
- **Živý stav** přes D-Bus a události Swaye (seznam Wi-Fi sítí, připojená zařízení, hlasitost),
  nic se nekontroluje dokola.
- **Hledání** nahoře v postranním panelu: hledá ve všech nastaveních (i podle klíčových slov),
  Enter skočí na nalezenou položku a krátce ji zvýrazní.
- **Klávesnice i touchpad**: Ctrl+F hledání, šipky v postranním panelu, Esc zavře dialog,
  velké plochy pro klikání.
- **Kam se ukládá**: tam, kam to patří (NetworkManager, BlueZ, `~/.config/apogee/settings.json`,
  nastavení Swaye do generovaného souboru `~/.config/sway/settings.conf`, který hlavní
  konfigurace načte). Záloha (`backup.sh`) to vše zachytí.
- **Chyby a potvrzení** krátkou lištou dole v okně („Connected to Home Wi-Fi“, „Wrong password“).

## Jak vypadá

- Okno asi 900 × 640, dá se zvětšit. Vlevo postranní panel (~220 px): hledání a seznam
  stránek s ikonami ve skupinách. Vpravo obsah stránky.
- Obsah jako v GNOME: zaoblené karty (barva `surface` z tématu) s řádky
  „název + popis vlevo, ovládací prvek vpravo“ (přepínač, posuvník, výběr, šipka na podstránku).
  Nadpis stránky nahoře, mezi kartami vzduch.
- Úzké okno (< 600 px): postranní panel se schová, stránky mají šipku zpět.

## Stránky

**Connections**
- **Wi-Fi**: zapnout / vypnout, seznam sítí (síla signálu, zámek), připojit s heslem, zapomenout,
  skrytá síť, režim letadlo; u připojené sítě IP adresa a automatické připojení.
- **Bluetooth**: zapnout / vypnout, spárovaná zařízení (připojit, odpojit, zapomenout, baterie
  zařízení), hledání a spárování nového (i potvrzení PINu).
- **Network**: kabel, VPN (import souboru), sdílení internetu (hotspot) – později.

**Device**
- **Sound**: výstup a vstup (výběr zařízení, hlasitost, ztlumit), hlasitost jednotlivých
  aplikací. Nahradí pavucontrol.
- **Display**: jas, noční režim (od–do / podle slunce, teplota), měřítko, rozlišení
  a obnovovací frekvence, externí monitor přes HDMI (rozmístění, zrcadlení).
- **Keyboard & Touchpad**: rozložení (cz / us a přepínání), opakování kláves, NumLock;
  ťukání, přirozené posouvání, rychlost kurzoru, gesta (co dělají 3 a 4 prsty).
- **Power & Battery**: režim (Power saver / Balanced / Performance), automatický úsporný režim
  na baterii (dnes dělá tuned), stav a opotřebení baterie, **conservation mode** (drží baterii
  kolem 60 % při stálém připojení, prodlouží jí život), za jak dlouho ztmavit / zamknout / uspat,
  co dělá zavření víka.

**Personalization**
- **Appearance**: tapeta (dnešní výběr tapet), světlý / tmavý vzhled, barvy podle tapety,
  velikost písma a kurzoru, tlačítka na liště oken.
- **Panel & Dock**: které moduly panel ukazuje, formát hodin, připnuté aplikace v doku
  (přetáhnout pro pořadí), schovávání doku.
- **Notifications**: Nerušit (i podle času), historie, ztlumit jednotlivé aplikace.
- **Shortcuts**: seznam všech klávesových zkratek a gest s hledáním, později i úpravy.

**System**
- **Apps**: výchozí aplikace (prohlížeč, video, soubory), aplikace po startu.
- **Updates**: dnešní okénko aktualizací (dnf + Flatpak) jako stránka, interval kontroly,
  stav upraveného Swaye (verze, poslední sestavení, tlačítko „Rebuild“).
- **Date & Time**: časové pásmo, 24 h, synchronizace času, formáty.
- **Storage**: co zabírá disk, vyčistit mezipaměti (dnf, nepoužívané Flatpak runtime, koš).
- **About**: počítač (procesor, paměť, grafika a jestli NVIDIA spí), verze, záloha
  (kdy naposledy, „Back up now“, kontrola obnovy `restore.sh --check`).

## Pořadí

1. Wi-Fi, Bluetooth, Sound, Power & Battery (to, kvůli čemu se dnes nejčastěji píše Claudovi).
2. Display, Keyboard & Touchpad, Appearance, Panel & Dock.
3. Zbytek.

Stávající okénka (wifi.rs, bluetooth.rs, zvuk.rs, baterie.rs, tapety.rs, aktualizace.rs)
už mají kód pro D-Bus a nastavování; aplikace ho použije, ne napíše znovu.
