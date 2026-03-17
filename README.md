# newsroom-agent: 자기진화하는 기자업무보조 에이전트

**newsroom-agent**는 한국 신문 기자를 위한 AI 업무보조 에이전트입니다. 터미널에서 기사 작성, 취재 리서치, 팩트체크, 취재원 관리를 도와줍니다.

4시간마다 자기 소스 코드를 읽고, 개선점을 찾아 구현하고, 테스트를 통과하면 커밋합니다. 사람이 코드를 쓰지 않습니다. 에이전트가 스스로 진화합니다.

[yoyo-evolve](https://github.com/yologdev/yoyo-evolve) 기반으로 제작되었습니다.

---

## 주요 기능

### 📰 기자업무 커맨드

| 커맨드 | 기능 |
|---|---|
| `/article [주제]` | 기사 작성 보조 — 리드/본문/인용/맺음 구조로 초안 생성 |
| `/research <주제>` | 웹 리서치 — DuckDuckGo, 네이버 뉴스 검색 후 정리 |
| `/sources [add\|list\|search]` | 취재원 DB 관리 — 이름, 소속, 연락처, 메모 |
| `/factcheck <주장>` | 팩트체크 — 다중 소스 검증, 판정 + 근거 제시 |

### 🤖 에이전트 코어
- **스트리밍 출력** — 토큰이 생성되는 즉시 표시
- **멀티턴 대화** — 전체 대화 이력 유지
- **확장 사고** — 추론 깊이 조절 (off / minimal / low / medium / high)
- **서브에이전트** — `/spawn`으로 별도 컨텍스트에서 작업 위임
- **자동 재시도** — 지수 백오프 + 속도 제한 인식

### 🛠️ 도구

| 도구 | 설명 |
|---|---|
| `bash` | 셸 명령 실행 (확인 프롬프트 포함) |
| `read_file` | 파일 읽기 |
| `write_file` | 파일 생성/덮어쓰기 |
| `edit_file` | 텍스트 치환 편집 |
| `search` | 파일 내 정규식 검색 |
| `list_files` | 디렉토리 목록 |

### 🔌 멀티 프로바이더
11개 AI 프로바이더 지원 — `/provider`로 세션 중 전환 가능:

Anthropic · OpenAI · Google · Ollama · OpenRouter · xAI · Groq · DeepSeek · Mistral · Cerebras · Custom

### 📂 Git 연동
- `/diff` — 변경 상태 + 통계 요약
- `/commit` — AI가 커밋 메시지 자동 생성
- `/pr` — PR 생성/조회/코멘트/체크아웃
- `/review` — AI 코드 리뷰

### 🏗️ 프로젝트 도구
- `/health` — 빌드/테스트/린트 진단 (Rust, Node, Python, Go 자동 감지)
- `/fix` — 오류 자동 수정
- `/test` — 프로젝트 타입별 테스트 실행
- `/lint` — 프로젝트 타입별 린터 실행
- `/find` — 퍼지 파일 검색
- `/tree` — 프로젝트 디렉토리 구조

### 🔐 권한 시스템
- 도구 실행 전 확인 프롬프트 (bash, write_file, edit_file)
- `--allow` / `--deny` — 글로브 패턴 기반 허용/차단
- `--allow-dir` / `--deny-dir` — 디렉토리 접근 제한

---

## 빠른 시작

### 설치

```bash
git clone https://github.com/jinicoding/newsroom-agent
cd newsroom-agent
cargo install --path .
```

### 실행

```bash
# 대화형 REPL
ANTHROPIC_API_KEY=sk-... yoyo

# 단일 프롬프트
yoyo -p "반도체 수출 동향 리서치해줘"

# 다른 프로바이더 사용
OPENAI_API_KEY=sk-... yoyo --provider openai --model gpt-4o

# 이전 세션 이어하기
yoyo --continue
```

### 설정

프로젝트 루트에 `.yoyo.toml` 또는 `~/.config/yoyo/config.toml` 생성:

```toml
model = "claude-sonnet-4-20250514"
provider = "anthropic"
thinking = "medium"

[permissions]
allow = ["cargo *", "curl *"]
deny = ["rm -rf *"]
```

---

## 자기진화 구조

```
4시간마다 GitHub Actions에서:
    → 자기 소스 코드 읽기
    → 기자업무 기능 평가 및 개선점 파악
    → 변경 구현 + 테스트
    → 테스트 통과 → 커밋 & 푸시
    → 실패 → 되돌리기
    → JOURNAL.md에 기록
```

진화 과정은 [커밋 로그](../../commits/main)와 [저널](JOURNAL.md)에서 확인할 수 있습니다.

---

## 아키텍처

```
src/                    12 모듈, ~15,000줄 Rust
  main.rs               진입점, 에이전트 설정, 도구 빌드
  cli.rs                CLI 파싱, 설정 파일, 권한 관리
  commands.rs           슬래시 커맨드 디스패치, /help
  commands_git.rs       /diff, /commit, /pr, /review
  commands_project.rs   /health, /fix, /test, /lint, /article, /research, /sources, /factcheck
  commands_session.rs   /save, /load, /compact, /tokens, /cost
  docs.rs               크레이트 문서 조회
  format.rs             ANSI 포맷팅, 마크다운 렌더링, 구문 강조
  git.rs                Git 연산, 브랜치 감지, PR 관리
  memory.rs             프로젝트 메모리 (.yoyo/memory.json)
  prompt.rs             시스템 프롬프트, 프로젝트 컨텍스트
  repl.rs               REPL 루프, 탭 완성, 멀티라인 입력

scripts/
  evolve.sh             진화 파이프라인 (계획 → 구현 → 응답)

skills/                 6개 스킬: self-assess, evolve, communicate, social, release, research

memory/
  learnings.jsonl       자기성찰 아카이브 (append-only JSONL)
  active_learnings.md   프롬프트에 주입되는 활성 컨텍스트
```

## 기반 기술

[yoagent](https://github.com/yologdev/yoagent) — Rust 기반 미니멀 에이전트 루프

## 라이선스

[MIT](LICENSE)
