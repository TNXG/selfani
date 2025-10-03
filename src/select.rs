use crate::playurl::{PlayAudio, PlayVideo};
use reqwest::Url;

fn codec_rank(codecid: Option<i32>) -> i32 { match codecid.unwrap_or_default() { 13 => 3, 12 => 2, 7 => 1, _ => 0 } }

pub fn select_best_video(videos: &[PlayVideo]) -> Option<&PlayVideo> {
    if videos.is_empty() { return None; }
    let mut idx: Vec<usize> = (0..videos.len()).collect();
    idx.sort_by(|&a, &b| {
        let va = &videos[a]; let vb = &videos[b];
        vb.id.cmp(&va.id)
            .then(codec_rank(vb.codecid).cmp(&codec_rank(va.codecid)))
            .then(vb.bandwidth.unwrap_or(0).cmp(&va.bandwidth.unwrap_or(0)))
            .then(vb.height.unwrap_or(0).cmp(&va.height.unwrap_or(0)))
    });
    idx.first().map(|&i| &videos[i])
}

pub fn select_best_audio(audios: &[PlayAudio]) -> Option<&PlayAudio> {
    if audios.is_empty() { return None; }
    let score = |id: i32| -> i32 { match id { 30252 => 1000, 30250 => 900, 30280 => 800, 30232 => 700, 30216 => 600, _ => 500 } };
    let mut idx: Vec<usize> = (0..audios.len()).collect();
    idx.sort_by(|&a, &b| { let aa = &audios[a]; let bb = &audios[b]; score(bb.id).cmp(&score(aa.id)).then(bb.bandwidth.unwrap_or(0).cmp(&aa.bandwidth.unwrap_or(0))) });
    idx.first().map(|&i| &audios[i])
}

pub fn url_filter(mut urls: Vec<String>) -> Vec<String> {
    if urls.is_empty() { return urls; }
    let mut mirror: Vec<Url> = vec![]; let mut upos: Vec<Url> = vec![]; let mut bcache: Vec<Url> = vec![]; let mut others: Vec<Url> = vec![];
    for u in urls.drain(..) { if let Ok(url) = Url::parse(&u) {
        let host = url.host_str().unwrap_or("").to_string();
        let os = url.query_pairs().find(|(k, _)| k == "os").map(|(_, v)| v.to_string()).unwrap_or_default();
        if host.contains("mirror") && os.ends_with("bv") { mirror.push(url); }
        else if os == "upos" { upos.push(url); }
        else if host.starts_with("cn") && os == "bcache" { bcache.push(url); }
        else { others.push(url); }
    }}
    if !mirror.is_empty() {
        let base = if mirror.len() < 2 { mirror.into_iter().chain(upos.into_iter()).collect::<Vec<_>>() } else { mirror };
        return base.into_iter().map(|u| u.into()).collect();
    }
    if !upos.is_empty() || !bcache.is_empty() {
        let list = if !upos.is_empty() { upos } else { bcache };
        let mirror_hosts = ["upos-sz-mirrorali.bilivideo.com", "upos-sz-mirrorcos.bilivideo.com"];
        return list.into_iter().enumerate().map(|(i, mut u)| {
            if let Some(host) = u.host_str().map(|s| s.to_string()) {
                let current = host; let fallback: &str = current.as_str();
                let chosen: &str = mirror_hosts.get(i).copied().unwrap_or(fallback);
                let _ = u.set_host(Some(chosen));
            }
            u.into()
        }).collect();
    }
    others.into_iter().map(|u| u.into()).collect()
}
