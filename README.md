# Nokevair

Nokevair is an experiment in worldbuilding, or procedural generation, or emergent narrative, or artificial life, or something.

The ultimate goal is to simulate a single world with a complex history over the course of many decades. New mechanics and features are incorporated into the existing simulation without restarting it, and the full history of the world is always available as public record. You can sift through the thousands of interconnected stories to find one that interests you.

While the project is still in its fairly early stages, I intend to put a lot of attention into politics, diplomacy, and economics. I want there to be wars, conquest, intrigue, and revolutions. I want users to be able to debate politics and predict the fates of imaginary nations. I want epic heroism and personal drama to be realized in equal detail. Hopefully, this can be realized in some form.

## Project Structure

The linchpin of the project is a Rust program called the *driver*. The driver is responsible for performing the simulation and acting as a web server serving pages that describe the project.

The driver depends on a number of other components:

- The *world state*: a series of [MessagePack](https://msgpack.org) objects that describes the state of the world at any given time.
- The *simulation*: a set of Lua scripts used to convert one version of the world state into the next.
- The *renderer*: a set of scripts used to describe various aspects of the world state at a particular time. Each entry in the renderer has two components:
  - The *focus*, written in Lua, which uses the query string to find the relevant portion of the world state and turn it into an object.
  - The *template*, written in [Tera](https://tera.netlify.app), which converts that object into an HTML document.

Before starting the driver, the locations of these resources must be specified in a config file. See [`example/Config.toml`](https://github.com/nokevair/nokevair/blob/master/example/Config.toml) for an example.

### World State

World state files have names like `360.msgpack` and are located in the directory labeled as `state` in the config file.

### Simulation

Simulation files have names like `foo.lua` and are located in the directory labeled as `sim` in the config file.

When performing the simulation, these files are evaluated, and their return value should be a function. This function is called with the following arguments:

- The world state, as a Lua object.
- The path to the file (e.g. `sim/foo.lua`).

### Renderer Entries

A renderer entry is a subdirectory of the directory labeled as `render` in the config file. Each entry should have the following files:

- `focus.lua`
- `format.tera`

When loading the renderer, each `focus.lua` script is evaluated, and its return value should be a function. When the server receives a request like `/360/person?i=3f91df`, it invokes the function defined by `person/focus.lua` with the following arguments:

- Version 360 of the world state, as a Lua object.
- The query parameter `"3f91df"`, or `nil` if no parameter was passed.

The return value of this function should be a table, which is interpreted as a Tera context and passed to `format.tera` to generate the page served to the user.

## Credits

The following external resources are used by this project:

- The [Carolingia](https://www.dafont.com/carolingia.font) font, used for the site logo.
- [Highlight.js](https://highlightjs.org)'s Lua syntax highlighting and the "Atelier Dune Light" theme.
- Brillout's [`sha256`](https://github.com/brillout/forge-sha256) implementation, used as `static/public/crypto.js`.
