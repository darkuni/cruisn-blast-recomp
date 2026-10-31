CRUIS'N BLAST - NATIVE PC BUILD
===============================

A native Windows version of Cruis'n Blast (Nintendo Switch, v2.0.0), made by static
recompilation. Not an emulator.
https://github.com/darkuni/cruisn-blast-recomp

This package contains no game files and no game code. setup makes the game on your own
computer from your own copy: it translates the game's program into PC code there. You need
your own copy of the game, dumped from your
own Switch:
  - the base game NSP          (title ID 0100B41013C82000)
  - the v2.0.0 update NSP      (title ID 0100B41013C82800, version 131072)
  - prod.keys from that Switch


WHAT YOU NEED
-------------
- Windows 10 or 11, 64-bit
- A GPU with Vulkan 1.3 support and current drivers
- About 6 GB of free disk space: 4 GB for the game data, 0.6 GB for the game, and up to
  1.5 GB more while setup runs


SETUP (ONCE)
------------
1. Unzip this folder anywhere.
2. Drag the two NSPs and prod.keys onto setup.exe.
   (Or copy them into this folder and double-click setup.exe. It also finds prod.keys in
   %USERPROFILE%\.switch\.)
3. Wait for "Done". It takes about five minutes:
   - it copies the game data with the update applied into game\ and pulls out the game's
     shaders
   - it translates the game's program into cruisn-blast.exe, using the recompiler and linker
     in the recompiler\ folder (this part uses every CPU core for a few minutes)
   The NSPs are only read, never changed. Nothing is downloaded and nothing leaves your PC.
4. Start cruisn-blast.exe.

After unpacking a newer release over this folder, run setup.exe again: it keeps the game data
and only makes cruisn-blast.exe again. Do not delete the recompiler\ folder.

STEAM DECK AND LINUX
--------------------
Use the Linux download. In a terminal in its folder: ./setup <base.nsp> <update.nsp> <prod.keys>
(or put them in the folder and run ./setup). It makes the cruisn-blast program. Add
cruisn-blast-linux.sh to Steam as a non-Steam game. Needs a Vulkan driver (SteamOS has one).

The first launch shows "PREPARING SHADERS" for a few seconds. That is the game's shaders
being compiled for your GPU so races don't stutter. Later launches are faster.


CONTROLS
--------
Xbox-style controllers work out of the box (XInput), with rumble.

Keyboard (Switch button = key):
  Steer               A / D or Left / Right
  Up / Down           W / S or Up / Down
  A                   Z or Space
  B                   X
  X / Y               C / V
  ZR / ZL             O / Q
  R / L               U / E
  +  (pause)          Enter
  -                   Backspace

Every key and button can be changed in the settings menu.

  F1 or Esc           settings menu (controller: Start + Back)
  F11 or Alt+Enter    fullscreen on / off


SETTINGS MENU
-------------
Resolution, aspect ratio, VSync, fullscreen, scaling, texture filtering, frame rate,
vibration and its strength, A/B and X/Y swap, frame rate display, keyboard keys,
controller buttons. Changes are saved to cruisn-blast.ini in this folder. Delete that file
to get the defaults back.

Your player name (kirikae unless you change it) is the "name =" line under [player] in
cruisn-blast.ini: up to 10 letters, digits, spaces or symbols. Edit it while the game is
closed. The file is made on the first launch.


KNOWN ISSUES
------------
- Short hitches can still happen during races, mostly in busy moments (takedowns, crowded
  sections). The Switch version drops frames in the same places.


REPORTING A PROBLEM
-------------------
Send these from this folder, plus what you were doing when it happened:
  cruisn-blast.log            what the game printed (rewritten on each launch, so copy it
                              before starting the game again)
  cruisn-blast.coverage.json  which system features the game asked for
  cruisn-blast.ini            your settings
Also say which GPU and driver version you have.

If the game crashes, a message box says so; the details are in cruisn-blast.log.

Unofficial fan project, not affiliated with Raw Thrills, GameMill or Nintendo. cruisn-blast.exe
is made on your computer from your own dump; none of the game's code or data is in this
download.
