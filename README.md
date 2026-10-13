# Lazy Ports

A COSMIC panel applet that manages local development services and ports —
a Linux/COSMIC port of the [LocalPorts](https://github.com/onderk-motion/LocalPorts)
macOS menu bar app.

## What it does

- **Listening Ports** — scans local TCP ports (`lsof`, falling back to `ss`)
  every 2 s and lists port, process and PID. Open a port in the browser,
  pin it as a service, or kill its process.
- **Pinned Services** — name + address + optional start command/working dir.
  Start/stop services (own process group via your login shell, SIGTERM →
  SIGKILL fallback), open in browser, view live logs (last 32 KiB of
  `$XDG_DATA_HOME/lazy-ports/logs/<id>.log`), edit and delete.
- **Health checks** — plain HTTP GET against each service address;
  200–399 counts as healthy, 1.5 s timeout.
- **Persistence** — service list is stored via cosmic-config
  (`services_json` key), survives restarts.

## Getting Started

```sh
just build-debug    # debug build
just run            # run for testing
just install        # install to /usr
```

Install from [casey/just][just]. Recipes mirror the COSMIC applet template.

## Project structure

```
src/
  main.rs      entry point (cosmic::applet::run)
  app.rs       applet model: panel button + popup UI
  config.rs    Config (cosmic-config) + ServiceConfig model
  ports.rs     listening-port scanner (lsof/ss parsing)
  services.rs  process manager, log tail, health checks
i18n/en/       fluent strings
resources/     desktop entry, metainfo, icon
```

## Documentation

Refer to the [libcosmic API documentation][api-docs] and [book][book].

[api-docs]: https://pop-os.github.io/libcosmic/cosmic/
[book]: https://pop-os.github.io/libcosmic-book/
[just]: https://github.com/casey/just

## Credits

Inspired by [LocalPorts](https://github.com/onderk-motion/LocalPorts).
Made proudly with vibes — the gates are coming down. A hayron & GLM Flash collaboration, inspired by LocalPorts.
