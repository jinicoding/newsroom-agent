## Session Plan

### Task 1: 출고 전 체크리스트 (/checklist) 신설
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: 기사 초안을 출고 전에 검증하는 `/checklist` 커맨드 추가. 파일 경로나 인라인 텍스트를 입력받아 AI가 다음 항목을 점검: (1) 육하원칙 충족 여부, (2) 출처 명시 확인, (3) 중립성/균형 보도 여부, (4) [확인 필요] 태그 잔존 확인, (5) 법적 리스크 (명예훼손, 초상권 등), (6) 숫자/날짜 일관성. 결과를 .journalist/checklist/에 저장. 테스트 작성 선행.
Issue: none

### Task 2: 취재원 DB에 분야(beat) 태그 추가
Files: src/commands_project.rs
Description: /sources add에 선택적 beat 필드 추가 (예: "경제", "정치", "IT"). /sources list에 beat 표시. /sources search가 beat로도 검색 가능하게. /sources beat <분야명>으로 해당 분야 취재원만 필터링. 기존 데이터 하위 호환성 유지 (beat 없으면 빈 문자열). 테스트 작성 선행.
Issue: none

### Task 3: /research search <키워드> 하위 명령 추가
Files: src/commands_project.rs
Description: 기존 /research list는 파일명만 나열. /research search <키워드>를 추가해서 저장된 리서치 파일의 내용까지 검색 가능하게. 파일명과 내용 모두 대소문자 무시 검색. 매칭된 파일의 제목줄과 미리보기 출력. 테스트 작성 선행.
Issue: none

### Task 4: 저널 엔트리 작성
Files: JOURNAL.md
Description: 이번 세션의 작업 결과를 저널에 기록. 무엇을 시도했고, 무엇이 작동했고, 다음에 무엇을 할지.
Issue: none
