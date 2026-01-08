# [![bonsai logo](readme/logo.png)](https://bonsai-framework.dev)

CLI for **bonsai**, a lightweight 2D game framework written in Odin.

## Installation

**If you have cargo installed, simply run:**

```bash
cargo install bonsai-cli
```

## Commands

| Command     | Usage                                  | Description                 |
| :---------- | :------------------------------------- | :-------------------------- |
| **init**    | `bonsai init <name> [options]`         | Create a new project        |
| **run**     | `bonsai run [dir] [options] [flags]`   | Compile and run the project |
| **build**   | `bonsai build [dir] [options] [flags]` | Compile the project         |
| **install** | `bonsai install <url> [options]`       | Install a game system       |
| **remove**  | `bonsai remove <name> [flags]`         | Remove a game system        |
| **docs**    | `bonsai docs <trigger> [options]`      | Generate reference files    |

---

### The `--verbose` flag

Every **bonsai** command also accepts a verbose flag, which has to stand **before** the called command, e.g.:
`bonsai --verbose build --web`

### `bonsai init`

Creates a new directory with the basic project structure.

**Usage:**
`bonsai init <name> [options]`

**Arguments:**

- `name`: Project name.

**Options:**

- `--dir`: Target parent directory. (default: '.')
- `--version`: Version (branch) of the framework. (default: latest)

**Example:**

```bash
bonsai init pot
```

### `bonsai run`

Runs a specified game project.

**Usage:**
`bonsai run [dir] [options] [flags]`

**Arguments:**

- `dir`: Project root directory. (default: '.')

**Options:**

- `--config`: Mode in which the game is run (debug/release). (default: debug)
- `--port`: Port used to open a server for the web build. (default: 8080)

**Flags:**

- `--desktop`: Runs the game in the desktop environment.
- `--web`: Opens a server and runs the game in the web browser.
- `--clean`: Recompiles/rebuilds every element of the game.

**If neither of desktop/web flags are selected, runs on desktop.**

**Example:**

```bash
bonsai run --web --port 8000
```

### `bonsai build`

Builds a **bonsai** project.

**Usage:**
`bonsai build [dir] [options] [flags]`

**Arguments:**

- `dir`: Project root directory. (default: '.')

**Options:**

- `--config`: Mode in which the game is run (debug/release). (default: debug)

**Flags:**

- `--desktop`: Builds the game for the desktop platform.
- `--web`: Builds the game for the web platform.
- `--clean`: Recompiles/rebuild every element of the game.

**If neither of desktop/web flags are selected, builds to desktop.**

**Example:**

```bash
bonsai build my_project --clean
```

### `bonsai install`

Installs a game system/module.

**Usage:**
`bonsai install <url> [options]`

**Arguments:**

- `url`: URL to the desired systems repository. Accepts the full URL or a \<username\>/\<repo_name\> syntax.

**Options:**

- `--version`: Version (branch) of the system. (default: latest)
- `--name`: Directory name for the system. (default: repo_name)

**Example:**

```bash
bonsai install nihiL7331/tween
```

### `bonsai remove`

Removes a game system/module.

**Usage:**
`bonsai remove <name> [flags]`

**Arguments:**

- `name`: Name of the removed system.

**Flags:**

- `--yes`: Skips the 'Are you sure ...?' segment.

### `bonsai docs`

Generates markdown reference docs of a project.
Looks for '@overview' and '\<trigger\>' in the comments of a file.
Generates markdown files using the comments and declarations below them.
Creates a mirrored structure of the project.
Supports **procedures**, **enums**, **structures**, **constants**, **unions** and **function overloads**.

**This function isn't well polished at all, hence bugs may occur while using it.**

**Usage:**
`bonsai docs <trigger> [options]`

**Arguments:**

- `trigger`: A 'trigger' the CLI looks for in the comments of a file. Marks the beginning of a referenced object.

**Options:**

- `--dir`: Root directory of a project. (default: '.')
- `--target`: Target directory of generated documents. (default: '.')

**Example:**

```bash
bonsai docs @ref --target ../website/docs
```

## Contributing

If you'd like to help build and expand the **bonsai** CLI, feel free to open an issue or PR!

![pot](readme/pot.gif)
