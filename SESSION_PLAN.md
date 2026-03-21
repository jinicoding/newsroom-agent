## Session Plan

Day 4 — 16:00 세션. 테마: **출고 이후 피드백 루프와 취재원 전략 관리**

### 자가 진단 결과

- 빌드/테스트: 통과 (67개 테스트, 0 실패)
- 현재 103개 커맨드(62개 기자 전용), 소스 약 529KB (commands_project.rs)
- 커뮤니티 이슈: 없음
- Day 3~4 저널에서 반복 언급한 "기사 퍼포먼스 추적"과 "취재원 네트워크 시각화"가 아직 미구현
- 자가 발견: 파이프라인이 "기사 출고"에서 끝남. 출고 이후 "이 기사가 어떤 반응을 얻었는가"를 추적하는 피드백 루프가 없음. 또한 /sources가 단순 주소록에 머물러 있어 전략적 취재원 네트워크 관리가 불가능.

### 전략적 판단

62개 기자 전용 커맨드로 취재→리서치→분석→작성→편집→출고→아카이브→협업→현황판까지 커버한다. 그러나 세 가지가 빠져 있다:

1. **기사 퍼포먼스 추적** — 기사를 내보낸 뒤 "잘 됐나?"를 확인할 방법이 없다. 조회수·댓글·공유 데이터를 기록하고 추세를 분석해야 다음 기사 전략을 세울 수 있다. 이건 Day 3부터 반복 언급한 최우선 과제다.
2. **취재원 네트워크 전략** — /sources는 이름·연락처 나열이 전부다. "내가 어느 분야 소스가 약한지", "이 주제를 취재하려면 누구를 만나야 하는지"를 파악하는 전략적 분석이 없다.
3. **AI 기사 아이디어 제안** — 기자의 매일 반복되는 고민인 "오늘 뭘 쓸까?"에 기존 취재 데이터를 활용해 맞춤형 제안을 할 수 있다.

---

### Task 1: /performance — 기사 퍼포먼스 추적
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: 기사 출고 후 성과를 기록·추적하는 커맨드. 하위 명령: `add <제목> --views N --comments N --shares N` (성과 데이터 등록), `update <번호> --views N` (업데이트), `list` (최근 기사별 성과 정렬 조회), `top` (베스트 성과 기사), `report` (주간/월간 퍼포먼스 리포트 — AI 사용). `.journalist/performance.json`에 JSON 배열로 저장. add/list/top/update는 AI 호출 없이 로컬 계산, report만 AI 사용. 기자에게 "어떤 기사가 잘 됐는지"는 다음 기사 전략의 핵심 입력이다. /archive가 기사 보관이라면, /performance는 기사 성과 보관이다. 테스트: CRUD, 정렬, JSON 직렬화.
Issue: none

### Task 2: /network — 취재원 네트워크 분석
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: /sources 데이터 기반 취재원 네트워크 분석 커맨드. 하위 명령: `map` (beat별 취재원 분포 매트릭스 — 어느 분야에 몇 명, 강/약 판단), `gaps` (취약 분야 식별 — beat가 비어있거나 소수인 분야 경고), `suggest <topic>` (특정 주제 취재에 필요한 취재원 유형 AI 제안). map/gaps는 AI 호출 없이 로컬 분석, suggest만 AI 사용. /sources가 주소록이라면 /network는 전략적 네트워크 분석 도구. 테스트: 분포 계산, 갭 탐지 로직.
Issue: none

### Task 3: /autopitch — AI 기사 아이디어 제안
Files: `src/commands_project.rs`, `src/commands.rs`, `src/repl.rs`
Description: `.journalist/` 아래 최근 취재 데이터(research, clips, trends, archive, sources)를 종합 분석해 기사 아이디어를 제안하는 커맨드. `--beat <분야>`로 출입처 맥락 지정 가능. AI가 (1) 최근 취재 주제에서 아직 다루지 않은 각도, (2) 후속 보도 기회, (3) 시의성 있는 주제를 제안한다. 결과는 `.journalist/pitches/`에 저장. "오늘 뭘 쓸까?"는 기자의 매일 반복되는 고민 — 기존 데이터를 활용한 맞춤 제안은 큰 가치. 테스트: 인자 파싱, 데이터 수집 로직.
Issue: none

### Task 4: 저널 작성
Files: `JOURNAL.md`
Description: 이번 세션의 작업 내용, 설계 판단, 파이프라인 현황, 다음 방향을 기록한다.
Issue: none
