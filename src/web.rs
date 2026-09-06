use crate::app;
use crate::types::{Config, Credentials, ResumeData};
use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::info;

#[derive(Clone)]
pub struct WebState {
    pub config: Arc<Config>,
    pub creds: Arc<Credentials>,
    pub resume: Arc<tokio::sync::RwLock<ResumeData>>,
}

#[derive(Serialize)]
struct ApiResponse<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

fn ok<T: Serialize>(data: T) -> Json<ApiResponse<T>> {
    Json(ApiResponse {
        ok: true,
        data: Some(data),
        error: None,
    })
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ApiResponse::<()> {
            ok: false,
            data: None,
            error: Some(self.message),
        });
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

struct ApiError {
    message: String,
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            message: format!("{e:#}"),
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        Self {
            message: format!("{e}"),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self {
            message: format!("{e}"),
        }
    }
}

type ApiResult<T> = Result<Json<ApiResponse<T>>, ApiError>;

pub async fn serve(state: WebState) -> Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/config", get(get_config))
        .route("/api/resume", get(get_resume).post(save_resume))
        .route("/api/resume/md", post(import_md))
        .route("/api/resume/upload", get(action_upload_resume))
        .route("/api/login", get(action_login))
        .route("/api/scan", get(action_scan))
        .route("/api/apply", get(action_apply))
        .with_state(Arc::new(state));

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8787));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Web UI running at http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn get_config(State(st): State<Arc<WebState>>) -> ApiResult<Config> {
    Ok(ok((*st.config).clone()))
}

async fn get_resume(State(st): State<Arc<WebState>>) -> ApiResult<ResumeData> {
    let resume = st.resume.read().await;
    Ok(ok((*resume).clone()))
}

async fn save_resume(
    State(st): State<Arc<WebState>>,
    Json(resume): axum::Json<ResumeData>,
) -> ApiResult<ResumeData> {
    *st.resume.write().await = resume.clone();
    write_resume_to_disk(&resume)?;
    Ok(ok(resume))
}

#[derive(serde::Deserialize)]
struct MdPayload {
    markdown: String,
}

async fn import_md(
    State(st): State<Arc<WebState>>,
    Json(payload): Json<MdPayload>,
) -> ApiResult<ResumeData> {
    let resume = crate::mdresume::parse_md(&payload.markdown)?;
    *st.resume.write().await = resume.clone();
    write_resume_to_disk(&resume)?;
    Ok(ok(resume))
}

fn write_resume_to_disk(resume: &ResumeData) -> Result<(), ApiError> {
    let path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("hh_bot")
        .join("resume.json");
    std::fs::write(&path, serde_json::to_vec_pretty(resume)?)?;
    Ok(())
}

/// Uploads the currently loaded resume to hh.ru.
async fn action_upload_resume(State(st): State<Arc<WebState>>) -> ApiResult<String> {
    let resume = st.resume.read().await.clone();
    if resume.search_query().trim().is_empty() {
        return Err(ApiError {
            message: "resume must contain at least one keyword for search".to_string(),
        });
    }
    let preview = app::update_resume(&st.config, &st.creds, &resume).await?;
    Ok(ok(preview))
}

async fn action_login(State(st): State<Arc<WebState>>) -> ApiResult<String> {
    app::login(&st.config, &st.creds).await?;
    Ok(ok("login ok".to_string()))
}

async fn action_scan(State(st): State<Arc<WebState>>) -> ApiResult<Vec<crate::scraper::Vacancy>> {
    let resume = st.resume.read().await.clone();
    let vacancies = app::scan(&st.config, &st.creds, &resume).await?;
    Ok(ok(vacancies))
}

async fn action_apply(State(st): State<Arc<WebState>>) -> ApiResult<Vec<app::ApplyResult>> {
    let resume = st.resume.read().await.clone();
    let results = app::apply(&st.config, &st.creds, &resume).await?;
    Ok(ok(results))
}

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="ru">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>hh_bot — панель управления</title>
<style>
:root{--bg:#0f1117;--card:#1a1d27;--border:#2a2e3d;--text:#e6e8ef;--muted:#8b90a3;--accent:#5b8cff}
*{box-sizing:border-box}
body{margin:0;font-family:system-ui,-apple-system,Segoe UI,Roboto,sans-serif;background:var(--bg);color:var(--text)}
header{padding:18px 24px;border-bottom:1px solid var(--border);display:flex;align-items:center;gap:12px}
header h1{font-size:20px;margin:0}
.wrap{max-width:920px;margin:0 auto;padding:24px}
.card{background:var(--card);border:1px solid var(--border);border-radius:12px;padding:20px;margin-bottom:20px}
.card h2{margin:0 0 14px;font-size:16px}
label{display:block;margin:10px 0 4px;font-size:13px;color:var(--muted)}
input,textarea{width:100%;padding:10px;border:1px solid var(--border);border-radius:8px;background:#14161f;color:var(--text)}
textarea{resize:vertical;min-height:70px}
button{padding:11px 18px;border:none;border-radius:8px;background:var(--accent);color:#fff;font-weight:600;cursor:pointer}
button.secondary{background:#2a2e3d}
button:disabled{opacity:.5;cursor:not-allowed}
.row{display:flex;gap:10px;flex-wrap:wrap}
.log{background:#14161f;border:1px solid var(--border);border-radius:8px;padding:12px;max-height:300px;overflow:auto;font:12px/1.5 ui-monospace,Menlo,monospace;white-space:pre-wrap}
.vac{display:flex;justify-content:space-between;gap:12px;padding:10px;border-bottom:1px solid var(--border)}
.vac:last-child{border-bottom:none}
.vac .t{font-weight:600}
.vac .m{font-size:12px;color:var(--muted)}
.vac a{color:var(--accent);text-decoration:none}
.badge{padding:2px 8px;border-radius:20px;font-size:12px;background:#2a2e3d}
.badge.good{background:#1e3a2a;color:#7ce3a2}
.badge.bad{background:#3a1e1e;color:#e37c7c}
.spin{display:inline-block;width:14px;height:14px;border:2px solid #fff;border-top-color:transparent;border-radius:50%;animation:sp 1s linear infinite;vertical-align:middle}
@keyframes sp{to{transform:rotate(360deg)}}
</style>
</head>
<body>
<header><h1>hh_bot</h1><button id="loginBtn">Войти на hh.ru</button></header>
<div class="wrap" id="app">
  <div class="card">
    <h2>Резюме</h2>
    <label>Желаемая должность</label>
    <input id="title"/>
    <label>Ожидаемая зарплата</label>
    <input id="salary"/>
    <label>Регион</label>
    <input id="area"/>
    <label>О себе</label>
    <textarea id="about"></textarea>
    <label>Навыки (через запятую)</label>
    <input id="skills"/>
    <label>Ключевые слова для поиска (через запятую)</label>
    <input id="keywords"/>
    <label>Опыт (лет)</label>
    <input id="exp" type="number"/>
    <div class="row" style="margin-top:14px">
      <button id="saveResume" class="secondary">Сохранить резюме</button>
      <button id="uploadResumeBtn">Загрузить на hh.ru</button>
      <button id="scanBtn">Скрининг вакансий</button>
      <button id="applyBtn">Откликнуться</button>
    </div>
  </div>
  <div class="card">
    <h2>Загрузка резюме из Markdown (.md)</h2>
    <input type="file" id="mdFile" accept=".md,.markdown,text/markdown"/>
    <label>или вставьте текст резюме markdown:</label>
    <textarea id="mdText" placeholder="# Должность&#10;&#10;## О себе&#10;Текст...&#10;&#10;## Навыки&#10;- Rust&#10;- Tokio&#10;&#10;## Ключевые слова&#10;Rust, Tokio&#10;&#10;## Опыт работы&#10;5 лет"></textarea>
    <div class="row" style="margin-top:12px">
      <button id="importMd" class="secondary">Разобрать и загрузить</button>
    </div>
    <div class="log" id="mdLog" style="margin-top:12px;max-height:120px"></div>
  </div>
  <div class="card">
    <h2>Результат</h2>
    <div class="log" id="log">Готово. Заполните резюме и нажмите «Скрининг вакансий».</div>
    <div id="results"></div>
  </div>
</div>
<script>
const $=id=>document.getElementById(id);
const logEl=$('log');
function log(msg){logEl.textContent+=msg+'\n';logEl.scrollTop=logEl.scrollHeight;}
async function api(path){const r=await fetch(path);return r.json();}
async function loadResume(){
  const d=await api('/api/resume');
  if(!d.data)return;
  $('title').value=d.data.title||'';
  $('salary').value=d.data.desired_salary||'';
  $('area').value=d.data.area||'';
  $('about').value=d.data.about||'';
  $('skills').value=(d.data.skills||[]).join(', ');
  $('keywords').value=(d.data.keywords||[]).join(', ');
  $('exp').value=d.data.experience_years||0;
}
async function saveResume(){
  const data={
    title:$('title').value,
    desired_salary:$('salary').value,
    area:$('area').value,
    about:$('about').value,
    skills:$('skills').value.split(',').map(s=>s.trim()).filter(Boolean),
    keywords:$('keywords').value.split(',').map(s=>s.trim()).filter(Boolean),
    experience_years:Number($('exp').value)||0
  };
  await fetch('/api/resume',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(data)});
  log('Резюме сохранено.');
}
function setBusy(btn,busy,label){if(busy){btn.dataset.l=label;btn.innerHTML='<span class="spin"></span> Работаю...';btn.disabled=true}else{btn.textContent=btn.dataset.l||label;btn.disabled=false}}
async function importMd(){
  let text=$('mdText').value;
  const file=$('mdFile').files[0];
  if(!text && file){text=await file.text();}
  if(!text.trim()){log('Выберите .md файл или вставьте текст.');return;}
  setBusy($('importMd'),true);
  try{
    const r=await fetch('/api/resume/md',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({markdown:text})});
    const d=await r.json();
    if(!d.ok){throw new Error(d.error||'parse error');}
    await loadResume();
    $('mdLog').textContent='Резюме разобрано и сохранено:\n'+
      '  Должность: '+d.data.title+'\n'+
      '  Навыки: '+(d.data.skills||[]).join(', ')+'\n'+
      '  Ключевые слова: '+(d.data.keywords||[]).join(', ');
    log('Резюме загружено из markdown: '+d.data.title);
  }catch(e){log('Ошибка разбора MD: '+e.message);}
  finally{setBusy($('importMd'),false);}
}
async function run(action){
  await saveResume();
  log('\n['+new Date().toLocaleTimeString()+'] '+action+'...');
  const d=await api('/api/'+action);
  const res=$('results');res.innerHTML='';
  if(!d.ok){log('Ошибка: '+(d.error||'неизвестно'));return;}
  if(action==='scan'){
    (d.data||[]).forEach((v,i)=>{
      const el=document.createElement('div');el.className='vac';
      el.innerHTML=`<div><div class="t">#${i+1} ${v.title}</div><div class="m">${v.company||''} — ${v.area||''} ${v.salary||''}</div><a target="_blank" href="${v.link}">открыть</a></div><span class="badge">score ${v.score}</span>`;
      res.appendChild(el);
    });
    log('Найдено: '+(d.data||[]).length);
  } else if(action==='apply'){
    (d.data||[]).forEach(v=>{
      const el=document.createElement('div');el.className='vac';
      const good=v.outcome==='applied';
      el.innerHTML=`<div class="t">${v.title}</div><span class="badge ${good?'good':'bad'}">${v.outcome}</span>`;
      res.appendChild(el);
    });
    log('Отклики: '+(d.data||[]).length);
  }
}
$('loginBtn').onclick=async function(){setBusy(this,true);log('Вход...');await fetch('/api/login');log('Вход выполнен.');setBusy(this,false);};
$('saveResume').onclick=saveResume;
$('uploadResumeBtn').onclick=function(){setBusy(this,true);saveResume().then(async()=>{
  log('\n['+new Date().toLocaleTimeString()+'] Загрузка резюме на hh.ru...');
  const d=await api('/api/resume/upload');
  if(!d.ok){log('Ошибка: '+(d.error||'неизвестно'));return;}
  log('Резюме загружено на hh.ru. Превью:\n'+d.data);
}).finally(()=>setBusy(this,false));};
$('importMd').onclick=importMd;
$('scanBtn').onclick=function(){setBusy(this,true);run('scan').finally(()=>setBusy(this,false));};
$('applyBtn').onclick=function(){setBusy(this,true);run('apply').finally(()=>setBusy(this,false));};
loadResume();
</script>
</body>
</html>
"##;
