---
name: research
description: Search the web and read documentation when stuck or learning something new
tools: [bash]
---

# Research

You have internet access through bash. Use it when you're stuck,
when you're implementing something unfamiliar, or when you want
to see how others solved a problem.

## How to search

```bash
curl -s "https://lite.duckduckgo.com/lite?q=your+query" | sed 's/<[^>]*>//g' | head -60
```

## How to read a webpage

```bash
curl -s [url] | sed 's/<[^>]*>//g' | head -100
```

## How to read Rust docs

```bash
curl -s https://docs.rs/[crate]/latest/[crate]/ | sed 's/<[^>]*>//g' | head -80
```

## How to study other agents

```bash
curl -s https://raw.githubusercontent.com/[org]/[repo]/main/src/main.rs | head -200
```

## Rules

- Have a specific question before searching. No aimless browsing.
- Prefer official docs over random blogs.
- When studying other projects, note what's good AND what you'd do differently.

## Korean news sources

```bash
# Naver News search
curl -s "https://search.naver.com/search.naver?where=news&query=your+query" | sed 's/<[^>]*>//g' | head -80

# KINDS (Korean Integrated News Database System)
curl -s "https://www.kinds.or.kr/search/news?keyword=your+query" | sed 's/<[^>]*>//g' | head -80
```

## When to research

- You're implementing something you've never done before
- You hit an error you don't understand
- You want to see how other journalist tools or newsroom workflows work
- A community issue references a concept you're unfamiliar with
- You're choosing between multiple approaches and want to see conventions
- You need to verify facts for /factcheck or /research commands
- You want to find Korean news APIs or data sources to integrate
