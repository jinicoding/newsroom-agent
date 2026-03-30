//! FOIA (정보공개청구) management command handlers.
//! Commands: /foia (file, list, status, update, remind)

use crate::commands_workflow::{
    datetime_to_epoch, day_of_week, format_date_from_epoch, today_date_string,
};
use crate::format::*;

// ── /foia — 정보공개청구 관리 ─────────────────────────────────────────

const FOIA_FILE: &str = ".journalist/foia/requests.json";

/// Subcommand names for `/foia <Tab>` completion.
pub const FOIA_SUBCOMMANDS: &[&str] = &["file", "list", "status", "update", "remind"];

/// Valid FOIA request statuses.
const FOIA_STATUSES: &[&str] = &["접수", "처리중", "연장", "응답완료", "이의신청", "거부"];

/// A single FOIA (정보공개청구) request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FoiaRequest {
    /// 1-based sequential ID
    pub(crate) id: u32,
    /// Target government agency / institution
    pub(crate) agency: String,
    /// Description of the information requested
    pub(crate) content: String,
    /// Current status
    pub(crate) status: String,
    /// Date the request was filed (YYYY-MM-DD)
    pub(crate) filed_date: String,
    /// Deadline date (10 business days from filing, YYYY-MM-DD)
    pub(crate) deadline_date: String,
}

pub(crate) fn foia_path() -> std::path::PathBuf {
    std::path::PathBuf::from(FOIA_FILE)
}

pub(crate) fn load_foia_requests(path: &std::path::Path) -> Vec<FoiaRequest> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("⚠ 정보공개청구 데이터 파싱 실패 ({}): {e}", path.display());
                Vec::new()
            }
        },
        _ => Vec::new(),
    }
}

fn save_foia_requests(requests: &[FoiaRequest], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(requests) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                eprintln!("⚠ 정보공개청구 데이터 저장 실패 ({}): {e}", path.display());
            }
        }
        Err(e) => eprintln!("⚠ 정보공개청구 데이터 직렬화 실패: {e}"),
    }
}

/// Calculate a date that is `business_days` business days (Mon-Fri) after the given
/// "YYYY-MM-DD" date string. Returns "YYYY-MM-DD".
fn add_business_days(start_date: &str, business_days: u32) -> Option<String> {
    // Validate the start date by checking day_of_week works
    day_of_week(start_date)?;

    let mut current = start_date.to_string();
    let mut remaining = business_days;

    while remaining > 0 {
        // Advance one calendar day using epoch arithmetic
        let epoch = datetime_to_epoch(&format!("{current}T00:00:00"))?;
        let next_epoch = epoch + 86400;
        current = format_date_from_epoch(next_epoch);

        // Check if it's a weekday (0=Mon .. 4=Fri)
        let dow = day_of_week(&current)?;
        if dow < 5 {
            remaining -= 1;
        }
    }
    Some(current)
}

/// Count calendar days between two YYYY-MM-DD dates (end - start). Can be negative.
fn calendar_days_between(start: &str, end: &str) -> Option<i64> {
    let to_days = |d: &str| -> Option<i64> {
        let p: Vec<i64> = d.split('-').filter_map(|s| s.parse().ok()).collect();
        if p.len() != 3 {
            return None;
        }
        let (year, month, day) = (p[0], p[1], p[2]);
        // Simple Julian day number approximation
        let mut total: i64 = 0;
        for y in 1..year {
            total += if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                366
            } else {
                365
            };
        }
        let days_in = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        for m in 1..month {
            total += days_in[m as usize];
            if m == 2 && ((year % 4 == 0 && year % 100 != 0) || year % 400 == 0) {
                total += 1;
            }
        }
        total += day;
        Some(total)
    };
    Some(to_days(end)? - to_days(start)?)
}

/// Handle the `/foia` command with subcommands: file, list, status, update, remind.
pub fn handle_foia(input: &str) {
    let args = input.strip_prefix("/foia").unwrap_or("").trim();

    if args.is_empty() {
        handle_foia_list_cmd(&foia_path());
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "file" => handle_foia_file_cmd(rest),
        "list" => handle_foia_list_cmd(&foia_path()),
        "status" => handle_foia_status_cmd(rest),
        "update" => handle_foia_update_cmd(rest),
        "remind" => handle_foia_remind_cmd(&foia_path()),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_foia_usage();
        }
    }
}

fn print_foia_usage() {
    println!("{DIM}  사용법:");
    println!("    /foia file <기관> <내용>     정보공개청구 등록 (응답기한 10영업일 자동계산)");
    println!("    /foia list                   진행 중/완료/지연 건 목록");
    println!("    /foia status <번호>          특정 청구 상세 (경과일, 남은 기한)");
    println!("    /foia update <번호> <상태>   상태 변경 (접수|처리중|연장|응답완료|이의신청|거부)");
    println!("    /foia remind                 응답 기한 임박/초과 건 알림");
    println!("    /foia                        (list와 동일){RESET}\n");
}

fn handle_foia_file_cmd(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /foia file <기관> <내용>{RESET}\n");
        return;
    }

    let (agency, content) = match args.split_once(char::is_whitespace) {
        Some((a, c)) => (a.trim(), c.trim()),
        None => {
            eprintln!("{RED}  기관과 내용을 모두 지정하세요: /foia file <기관> <내용>{RESET}\n");
            return;
        }
    };

    if content.is_empty() {
        eprintln!("{RED}  내용을 지정하세요: /foia file <기관> <내용>{RESET}\n");
        return;
    }

    let path = foia_path();
    let mut requests = load_foia_requests(&path);

    let next_id = requests.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    let today = today_date_string();
    let deadline = add_business_days(&today, 10).unwrap_or_else(|| "계산실패".to_string());

    let req = FoiaRequest {
        id: next_id,
        agency: agency.to_string(),
        content: content.to_string(),
        status: "접수".to_string(),
        filed_date: today.clone(),
        deadline_date: deadline.clone(),
    };

    requests.push(req);
    save_foia_requests(&requests, &path);

    println!("{GREEN}  📋 정보공개청구 등록 (#{next_id}){RESET}");
    println!("     기관: {agency}");
    println!("     내용: {content}");
    println!("     접수일: {today}");
    println!("     응답기한: {deadline} (10영업일)\n");
}

fn handle_foia_list_cmd(path: &std::path::Path) {
    let requests = load_foia_requests(path);

    if requests.is_empty() {
        println!("{DIM}  등록된 정보공개청구가 없습니다.{RESET}\n");
        return;
    }

    let today = today_date_string();

    println!("{BOLD}  📋 정보공개청구 목록{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");

    for req in &requests {
        let overdue = is_foia_overdue(req, &today);
        let done = req.status == "응답완료" || req.status == "거부";

        let icon = if done {
            format!("{GREEN}✅{RESET}")
        } else if overdue {
            format!("{RED}🔴{RESET}")
        } else {
            format!("{YELLOW}⏳{RESET}")
        };

        let status_str = if overdue && !done {
            format!("{RED}{}{RESET}", req.status)
        } else if done {
            format!("{GREEN}{}{RESET}", req.status)
        } else {
            req.status.clone()
        };

        println!(
            "  {icon} [{}] {} — {} [{status_str}] (기한: {})",
            req.id, req.agency, req.content, req.deadline_date
        );
    }
    println!();
}

fn handle_foia_status_cmd(num_str: &str) {
    if num_str.is_empty() {
        eprintln!("{RED}  번호를 지정하세요: /foia status <번호>{RESET}\n");
        return;
    }

    let id: u32 = match num_str.trim().parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{RED}  유효한 번호를 지정하세요: /foia status <번호>{RESET}\n");
            return;
        }
    };

    let path = foia_path();
    let requests = load_foia_requests(&path);

    let req = match requests.iter().find(|r| r.id == id) {
        Some(r) => r,
        None => {
            eprintln!("{RED}  #{id}에 해당하는 청구가 없습니다.{RESET}\n");
            return;
        }
    };

    let today = today_date_string();
    let elapsed = calendar_days_between(&req.filed_date, &today).unwrap_or(0);
    let remaining = calendar_days_between(&today, &req.deadline_date).unwrap_or(0);
    let overdue = is_foia_overdue(req, &today);
    let done = req.status == "응답완료" || req.status == "거부";

    println!("{BOLD}  📋 정보공개청구 #{}{RESET}", req.id);
    println!("     기관: {}", req.agency);
    println!("     내용: {}", req.content);
    println!("     상태: {}", req.status);
    println!("     접수일: {}", req.filed_date);
    println!("     응답기한: {}", req.deadline_date);
    println!("     경과일: {elapsed}일");
    if done {
        println!("     {GREEN}완료됨{RESET}");
    } else if overdue {
        println!("     {RED}⚠ 기한 초과 ({days}일){RESET}", days = -remaining);
    } else {
        println!("     남은 기한: {remaining}일");
    }
    println!();
}

fn handle_foia_update_cmd(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /foia update <번호> <상태>{RESET}");
        eprintln!("{DIM}  상태: 접수, 처리중, 연장, 응답완료, 이의신청, 거부{RESET}\n");
        return;
    }

    let (num_str, new_status) = match args.split_once(char::is_whitespace) {
        Some((n, s)) => (n.trim(), s.trim()),
        None => {
            eprintln!("{RED}  번호와 상태를 모두 지정하세요: /foia update <번호> <상태>{RESET}\n");
            return;
        }
    };

    let id: u32 = match num_str.parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{RED}  유효한 번호를 지정하세요: /foia update <번호> <상태>{RESET}\n");
            return;
        }
    };

    if !FOIA_STATUSES.contains(&new_status) {
        eprintln!("{RED}  유효하지 않은 상태: {new_status}{RESET}");
        eprintln!(
            "{DIM}  사용 가능한 상태: {}{RESET}\n",
            FOIA_STATUSES.join(", ")
        );
        return;
    }

    let path = foia_path();
    let mut requests = load_foia_requests(&path);

    let req = match requests.iter_mut().find(|r| r.id == id) {
        Some(r) => r,
        None => {
            eprintln!("{RED}  #{id}에 해당하는 청구가 없습니다.{RESET}\n");
            return;
        }
    };

    let old_status = req.status.clone();

    // If extended ("연장"), push deadline by 10 more business days from current deadline
    if new_status == "연장" {
        if let Some(new_deadline) = add_business_days(&req.deadline_date, 10) {
            req.deadline_date = new_deadline;
        }
    }

    req.status = new_status.to_string();
    save_foia_requests(&requests, &path);

    println!("{GREEN}  ✅ #{id} 상태 변경: {old_status} → {new_status}{RESET}");
    if new_status == "연장" {
        if let Some(r) = requests.iter().find(|r| r.id == id) {
            println!("     새 응답기한: {}", r.deadline_date);
        }
    }
    println!();
}

fn handle_foia_remind_cmd(path: &std::path::Path) {
    let requests = load_foia_requests(path);
    let today = today_date_string();

    let active: Vec<&FoiaRequest> = requests
        .iter()
        .filter(|r| r.status != "응답완료" && r.status != "거부")
        .collect();

    if active.is_empty() {
        println!("{DIM}  활성 정보공개청구가 없습니다.{RESET}\n");
        return;
    }

    let mut overdue = Vec::new();
    let mut imminent = Vec::new(); // 3일 이내

    for req in &active {
        let remaining = calendar_days_between(&today, &req.deadline_date).unwrap_or(0);
        if remaining < 0 {
            overdue.push((*req, remaining));
        } else if remaining <= 3 {
            imminent.push((*req, remaining));
        }
    }

    if overdue.is_empty() && imminent.is_empty() {
        println!("{GREEN}  ✅ 기한 임박/초과 건이 없습니다.{RESET}\n");
        return;
    }

    if !overdue.is_empty() {
        println!("{RED}{BOLD}  ⚠ 기한 초과{RESET}");
        for (req, remaining) in &overdue {
            println!(
                "  {RED}🔴 [{}] {} — {} ({}일 초과){RESET}",
                req.id,
                req.agency,
                req.content,
                -remaining
            );
        }
    }

    if !imminent.is_empty() {
        println!("{YELLOW}{BOLD}  ⏰ 기한 임박 (3일 이내){RESET}");
        for (req, remaining) in &imminent {
            println!(
                "  {YELLOW}🟡 [{}] {} — {} ({}일 남음){RESET}",
                req.id, req.agency, req.content, remaining
            );
        }
    }
    println!();
}

/// Check if a FOIA request is overdue given today's date.
fn is_foia_overdue(req: &FoiaRequest, today: &str) -> bool {
    if req.status == "응답완료" || req.status == "거부" {
        return false;
    }
    calendar_days_between(today, &req.deadline_date)
        .map(|d| d < 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod foia_tests {
    use super::*;

    #[test]
    fn foia_subcommands_constant() {
        assert!(FOIA_SUBCOMMANDS.contains(&"file"));
        assert!(FOIA_SUBCOMMANDS.contains(&"list"));
        assert!(FOIA_SUBCOMMANDS.contains(&"status"));
        assert!(FOIA_SUBCOMMANDS.contains(&"update"));
        assert!(FOIA_SUBCOMMANDS.contains(&"remind"));
        assert_eq!(FOIA_SUBCOMMANDS.len(), 5);
    }

    #[test]
    fn add_business_days_skips_weekends() {
        // 2026-03-23 is Monday
        // 10 business days from Monday = 2 full weeks later = 2026-04-06 (Monday)
        let result = add_business_days("2026-03-23", 10);
        assert_eq!(result, Some("2026-04-06".to_string()));
    }

    #[test]
    fn add_business_days_from_friday() {
        // 2026-03-27 is Friday
        // 1 business day later = Monday 2026-03-30
        let result = add_business_days("2026-03-27", 1);
        assert_eq!(result, Some("2026-03-30".to_string()));
    }

    #[test]
    fn add_business_days_zero() {
        let result = add_business_days("2026-03-23", 0);
        assert_eq!(result, Some("2026-03-23".to_string()));
    }

    #[test]
    fn add_business_days_invalid_date() {
        assert!(add_business_days("bad-date", 5).is_none());
    }

    #[test]
    fn calendar_days_between_same_day() {
        assert_eq!(calendar_days_between("2026-03-28", "2026-03-28"), Some(0));
    }

    #[test]
    fn calendar_days_between_forward() {
        assert_eq!(calendar_days_between("2026-03-28", "2026-04-07"), Some(10));
    }

    #[test]
    fn calendar_days_between_backward() {
        assert_eq!(calendar_days_between("2026-04-07", "2026-03-28"), Some(-10));
    }

    #[test]
    fn foia_file_and_list() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("requests.json");

        // Initially empty
        let requests = load_foia_requests(&path);
        assert!(requests.is_empty());

        // Save a request
        let req = FoiaRequest {
            id: 1,
            agency: "국토교통부".to_string(),
            content: "서울시 공시지가 산출근거 자료".to_string(),
            status: "접수".to_string(),
            filed_date: "2026-03-28".to_string(),
            deadline_date: "2026-04-13".to_string(),
        };
        save_foia_requests(&[req], &path);

        let loaded = load_foia_requests(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].agency, "국토교통부");
        assert_eq!(loaded[0].status, "접수");
    }

    #[test]
    fn foia_status_update() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("requests.json");

        let req = FoiaRequest {
            id: 1,
            agency: "환경부".to_string(),
            content: "폐기물 처리 현황".to_string(),
            status: "접수".to_string(),
            filed_date: "2026-03-20".to_string(),
            deadline_date: "2026-04-03".to_string(),
        };
        save_foia_requests(&[req], &path);

        let mut requests = load_foia_requests(&path);
        requests[0].status = "처리중".to_string();
        save_foia_requests(&requests, &path);

        let reloaded = load_foia_requests(&path);
        assert_eq!(reloaded[0].status, "처리중");
    }

    #[test]
    fn foia_overdue_detection() {
        let req = FoiaRequest {
            id: 1,
            agency: "국방부".to_string(),
            content: "무기 도입 계약서".to_string(),
            status: "접수".to_string(),
            filed_date: "2026-01-01".to_string(),
            deadline_date: "2026-01-15".to_string(),
        };

        // Today is well past the deadline
        assert!(is_foia_overdue(&req, "2026-03-28"));
        // Before deadline
        assert!(!is_foia_overdue(&req, "2026-01-10"));
        // Completed requests are never overdue
        let done = FoiaRequest {
            status: "응답완료".to_string(),
            ..req.clone()
        };
        assert!(!is_foia_overdue(&done, "2026-03-28"));
    }

    #[test]
    fn foia_deadline_calculation_10_business_days() {
        // From 2026-03-28 (Saturday) — next business day progression
        // 2026-03-28 is Saturday, so:
        // Day 1: Mon 3/30, Day 2: Tue 3/31, Day 3: Wed 4/1, Day 4: Thu 4/2, Day 5: Fri 4/3
        // Day 6: Mon 4/6, Day 7: Tue 4/7, Day 8: Wed 4/8, Day 9: Thu 4/9, Day 10: Fri 4/10
        let result = add_business_days("2026-03-28", 10);
        assert_eq!(result, Some("2026-04-10".to_string()));
    }

    #[test]
    fn foia_valid_statuses() {
        assert!(FOIA_STATUSES.contains(&"접수"));
        assert!(FOIA_STATUSES.contains(&"처리중"));
        assert!(FOIA_STATUSES.contains(&"연장"));
        assert!(FOIA_STATUSES.contains(&"응답완료"));
        assert!(FOIA_STATUSES.contains(&"이의신청"));
        assert!(FOIA_STATUSES.contains(&"거부"));
        assert_eq!(FOIA_STATUSES.len(), 6);
    }

    #[test]
    fn foia_day_of_week_known_dates() {
        // 2026-03-28 is Saturday → should be 5
        assert_eq!(day_of_week("2026-03-28"), Some(5));
        // 2026-03-23 is Monday → should be 0
        assert_eq!(day_of_week("2026-03-23"), Some(0));
        // 2026-03-29 is Sunday → should be 6
        assert_eq!(day_of_week("2026-03-29"), Some(6));
    }
}
