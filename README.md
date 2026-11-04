# Geniuz

Your AI starts every conversation from zero. Geniuz keeps what matters, the calls you made and the reasons behind them, on your own computer, where any AI you use can find it.

Free and open source. No account, no cloud, no telemetry.

## Install

| You have | Do this |
|---|---|
| A Mac (Apple Silicon, Sonoma or later) | [Download Geniuz.dmg](https://github.com/jackccrawford/Geniuz/releases/latest/download/Geniuz.dmg) and drag it to Applications |
| Windows 11 | [Download Geniuz-Setup.exe](https://github.com/jackccrawford/Geniuz/releases/latest/download/Geniuz-Setup.exe) |
| Linux (x86_64 or arm64) | Run the command below |

```bash
curl -fsSL https://raw.githubusercontent.com/jackccrawford/geniuz/main/install.sh | bash
```

## Connect your AI

| You use | Do this |
|---|---|
| Claude Desktop | **Configure Claude Connection** in the menubar, or `geniuz mcp install`, then restart Claude Desktop |
| Anything else that speaks MCP | Add `geniuz mcp serve` as a server |
| Anything that can run a command | `geniuz skill` prints a guide to give your agent |

Then keep talking. Your AI saves what matters as you go. Tomorrow, in a new chat, ask it what you told it today.

## Try it

```bash
geniuz keep -c "Acme wants invoices rounded down. They failed an audit over it in 2024." -g "client: Acme rounding"
geniuz recall "how does Acme want amounts rounded"

geniuz folder create Work -d "Client projects. No passwords here."
geniuz keep --in Work -c "Dana is the contact at Acme." -g "client: Acme contact"
geniuz recall --all "who do I talk to at Acme"
```

The first `keep` downloads a small model (about 118 MB) that runs on your computer.

## What makes it different

- **It's yours.** Your memories are files on your computer, and search runs there too.
- **It isn't tied to one AI.** Change tools and your memory comes with you.
- **Nothing is overwritten.** A correction is added beside the original, so you can trust what you find.
- **It finds things two ways.** By meaning, for what you can't quite name, and exactly, for names and numbers.
- **Folders keep things apart.** Work, Home, one per project, or a place agents leave each other messages.

## The idea, if you only take one thing

One table: a UUID, a JSON payload, a parent, a time. Rows are added and never changed. That shape holds far more kinds of work than you'd expect, and it spares you most of the design a normalized schema demands up front.

Borrow it and tailor it. Make something updatable only where a concrete need says so, and write that need down beside the schema. Ours is [`schema/schema.sql`](schema/schema.sql).

## Learn more

- **[The wiki](https://geniuz.life/wiki/)** explains how it works, every command, and why it's built this way.
- **[geniuz.life](https://geniuz.life)** is the short version, and [the longer story](https://geniuz.life/why/).
- **[Contributing](CONTRIBUTING.md)**, [issues](https://github.com/jackccrawford/Geniuz/issues) and [discussions](https://github.com/jackccrawford/Geniuz/discussions).

MIT licensed. See [LICENSE](LICENSE).
