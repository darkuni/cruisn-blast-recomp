@echo off
rem Runs the game with GPU checkpoints: if the graphics driver stops the GPU (device lost),
rem cruisn-blast.log then says which draw it was on. A little slower; only for reporting a crash.
cd /d "%~dp0"
set KIRIKAE_GPU_DIAG=1
start "" cruisn-blast.exe
