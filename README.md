# STStat
Windows Sidebar showing various system information written in rust 🦀, inspired by various Windows Vista / 7 Gadgets.

![Preview](https://raw.githubusercontent.com/chrisheib/STStat/main/screenshot/desktop-main.jpg)

Needs [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) to run in the background and its web server to be started on port 8085:

![Preview](https://raw.githubusercontent.com/chrisheib/STStat/main/screenshot/lhm.jpg)

## Goals
* 💻 Provide an overview of your computers ressources.
* ✅ Focus on stats that provide actual value. 
* 🚀 Don't generate much load. STStat is developed as replacement for Windows Gadgets that were running in Webviews rendered with HTML and JS, and should always be easier on the battery and general ressource usage. (If you see usage above 0.5% for the ststat.exe process, please use 'Settings -> trace perf' to trace the actual performance, save a report a few seconds later with 'save trace', and make sure to include the resulting timings.txt in your issue!)
* 🪟 Use the Windows API to look and feel like a true native windows sidebar, including limiting the space of maximised windows and not showing up in the task switcher.

## Limitations
* Needs to be run as Admin to view the usage of system processes. Will still work fine without being run as admin, but can't show the load of processes like Windows Defender and most services.
* Not yet tested on AMD CPUs and GPUs, super limited testing in general. If you run it successfully (or run into errors - please attach the errors.txt, if present) please do give feedback!
* Only runs on Windows (tested on Win 10 and Win 11). Most of the functions directly query the Windows API.
* Kinda depends on LibreHardwareMonitor to be useful. I tried implementing most of the stat readouts from scratch, but couldn't easily get performance comparable to that of LHWM. As I need that for the temperature readouts anyway, I relied on it a bit more than necessary. 

## Feature Overview
* CPU and RAM:
  * CPU usage
  * CPU temp (°C)
  * RAM usage
  * Core load
* GPU
  * GPU usage
  * VRAM usage and total
  * Power usage and target
  * Core clock
  * GPU temp (°C)
* Drives
  * Free Disk space 
  * Disk usage time
* Net (Configurable via settings)
  * Up and Down traffic
* Ping
  * Ping google once a second. 
  * M: Max ping in the last 2 minutes
  * C: Time of last ping response
* Processes (Doubleclick to open task manager!)
  * Group processes by name (x10 -> 10 processes with this name)
  * show top CPU and top RAM processes
  * reading process data is expensive -> slower refresh 
* Settings
  * Show Networks: Check every network adapter you want to monitor
  * Screen ID: On which screen the sidebar should be displayed
  * Display on right side: Check to move the sidebar to the right screen edge
  * trace perf / save trace: Keep an internal record of performance timings. Click 'save' after tracing for a few seconds to save it into a timings.txt in the folder containing the exe and include the file in a new issue to help me debug performance issues!