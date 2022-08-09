# ydnc-time [![GitHub license]][license] [![GitHub top language]]() [![GitHub latest release]][releases]

[github license]: https://img.shields.io/github/license/jming422/ydnc-time
[license]: https://github.com/jming422/ydnc-time/blob/main/LICENSE
[github top language]: https://img.shields.io/github/languages/top/jming422/ydnc-time
[github latest release]: https://img.shields.io/github/v/release/jming422/ydnc-time
[releases]: https://github.com/jming422/ydnc-time/releases

**Y**ou **D**on't **N**eed the **C**loud to log your time

---

This is software to help you track how much time you spend on different types of activities. It is highly opinionated, very minimal, entirely local, and terminal only.

![screenshot](https://raw.githubusercontent.com/jming422/ydnc-time/main/screenshots/screenshot1.png)

## Main Features

- Cross-platform
- Works well even with very low terminal resolution
- Autosaves time log to a plain-text format ([RON](https://github.com/ron-rs/ron) to be specific)
  - Saves to a directory in your `$XDG_CONFIG_HOME` (if applicable) or else in `$HOME/.ydnc/time`
- Controllable using the keyboard or using a [Timeular Tracker](https://timeular.com/tracker/)

## But why though

Because I have found it's really helpful for me to know the percentage of my time I spend on one type of work over another throughout my day and my most recent week. It's especially useful when I find I'm not doing the type of work I want to be doing at work, because then I can identify the activities that are distracting me and make concrete changes to spend less time on them.

And I became annoyed at how slow the UI was on my previous (cloud-backed) time tracking software choice, so I decided to write my own and make it fast!

## What's YDNC?

"You Don't Need the Cloud" ;)

This came out of the fact that until writing this app, I'd been using cloud-based time tracking software, but I found that it was:

- Slow. The UI felt sluggish to me, with a long startup time.
- Expensive. Using the software _at all_ required a subscription.
- Unnecessary. I only used this software on one computer; why was it even on the cloud?

So, after a friend suggested the idea that I write my own minimalist version, I decided to do just that! And `ydnc-time` was born as an opinionated, cloudless, and _fast_ alternative for my own time tracking.

Maybe someday I'll think of other stuff YDNC for and make some of those apps too :)

## But what if I want the cloud

Then these aren't the ~droids~ software you're looking for -- there are plenty of legitimate use cases for cloud-backed software, and maybe you really do want those features like collaboration, syncing, and the like, and that's great!

YDNC applications are less "anti-cloud" and more "do I _have_ to use the cloud?" For some people and workflows, the cloud is simply not necessary, and including it introduces application latency and other undesirable side effects. If you find you're not even using the cloud features like collaboration or cross-device syncing, then why not try removing the cloud from the equation? You'll probably be surprised at how performant software can be when it's running on your own computer!

I suppose, if you really wanted, you could locate where YDNC stores its save files on disk and somehow sync that folder to the cloud storage of your choice. But that's not in scope for this project, so I'll leave it as an exercise for the reader.

## References

I wouldn't have been able to get this app working if it weren't for these other great open-source works:

- [tui-rs's input example](https://github.com/fdehau/tui-rs/blob/master/examples/user_input.rs)
- [btleplug's subscribe example](https://github.com/deviceplug/btleplug/blob/master/examples/subscribe_notify_characteristic.rs)
- [@lemariva](https://github.com/lemariva)'s work on interfacing with the Timeular Tracker in Python:
  - https://lemariva.com/blog/2020/04/timeular-track-your-time-using-octahedron-linux
  - https://github.com/lemariva/timeular-python
- [@codingforfun](https://github.com/codingforfun)'s experimentation notes on the Timeular Tracker/Zei°: https://github.com/codingforfun/zeipy

## Disclaimer

This project is not affiliated with Timeular GmbH. References within this project to a Bluetooth "Tracker" or "Timeular Tracker" refer to Timeular's fantastic piece of hardware, which you can find more info about [here](https://timeular.com/tracker/).
