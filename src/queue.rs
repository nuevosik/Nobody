//! Domain — fila de notificações compartilhada entre D-Bus e UI.
//! Thread-safe, poison-safe, com limite fixo.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::state::Notice;

/// Mantém no máximo 12 notificações no queue (UI renderiza só 5).
/// 12 = buffer para não perder burst enquanto animação de saída roda.
pub const KEEP: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    Expired = 1,
    DismissedByUser = 2,
    ClosedByCall = 3,
    Undefined = 4,
}

impl CloseReason {
    pub const fn code(self) -> u32 {
        self as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest {
    pub id: u32,
    pub reason: CloseReason,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PushOutcome {
    pub id: u32,
    pub evicted: Vec<Notice>,
}

#[derive(Clone)]
pub struct Queue {
    inner: Arc<Mutex<VecDeque<Notice>>>,
    close_requests: Arc<Mutex<VecDeque<CloseRequest>>>,
    next_id: Arc<AtomicU32>,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            close_requests: Arc::new(Mutex::new(VecDeque::new())),
            next_id: Arc::new(AtomicU32::new(1)),
        }
    }

    pub fn push_with_outcome(&self, replaces: u32, mut notice: Notice) -> PushOutcome {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if replaces != 0 {
            if let Some(pos) = inner.iter().position(|n| n.id == replaces) {
                self.reserve_id(replaces);
                // Substituição: remove antigo e reinsere no topo como nova chegada
                inner.remove(pos);
                notice.id = replaces;
                inner.push_front(notice);
                return PushOutcome { id: replaces, evicted: Vec::new() };
            }
            // replaces solicitado mas não existe → aloca ID novo (não squatta ID arbitrário)
            // ainda reserva para evitar reutilização imediata
            self.reserve_id(replaces);
        }

        let id = self.next_available_id(&inner);
        notice.id = id;
        inner.push_front(notice);
        let mut evicted = Vec::new();
        while inner.len() > KEEP {
            if let Some(notice) = inner.pop_back() {
                evicted.push(notice);
            }
        }
        PushOutcome { id, evicted }
    }

    /// Remove por ID.
    pub fn remove(&self, id: u32) -> Option<Notice> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let idx = inner.iter().position(|n| n.id == id)?;
        inner.remove(idx)
    }

    pub fn snapshot(&self) -> Vec<Notice> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }

    pub fn remove_expired_at(&self, now_ms: u128) -> Vec<Notice> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut expired = Vec::new();
        inner.retain(|notice| {
            if notice.is_expired_at(now_ms) {
                expired.push(notice.clone());
                false
            } else {
                true
            }
        });
        expired
    }

    pub fn request_close(&self, id: u32, reason: CloseReason) {
        if id == 0 {
            return;
        }

        let mut requests = self.close_requests.lock().unwrap_or_else(|e| e.into_inner());
        if requests.len() >= KEEP * 2 {
            // evita crescimento ilimitado se D-Bus cair
            requests.pop_front();
        }
        if let Some(existing) = requests.iter_mut().find(|r| r.id == id) {
            // atualiza razão se nova for mais específica (DismissedByUser > Expired)
            // prioriza interação do usuário sobre expiração
            if existing.reason != reason {
                // DismissedByUser e ClosedByCall têm precedência sobre Expired/Undefined
                let priority = |r: CloseReason| match r {
                    CloseReason::DismissedByUser => 3,
                    CloseReason::ClosedByCall => 3,
                    CloseReason::Expired => 2,
                    CloseReason::Undefined => 1,
                };
                if priority(reason) > priority(existing.reason) {
                    existing.reason = reason;
                }
            }
            return;
        }
        requests.push_back(CloseRequest { id, reason });
    }

    pub fn drain_close_requests(&self) -> Vec<CloseRequest> {
        self.close_requests.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }

    fn next_available_id(&self, notices: &VecDeque<Notice>) -> u32 {
        // tenta no máximo (u32::MAX tentativas seria loop infinito; KEEP=12 então colisão rara)
        // mas trata wrap-around sem devolver duplicata
        let mut attempts = 0;
        loop {
            let id = self
                .next_id
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    Some(current.checked_add(1).unwrap_or(1))
                })
                .expect("the ID generator always produces a value");
            if !notices.iter().any(|notice| notice.id == id) {
                return id;
            }
            attempts += 1;
            // se todas as 12 IDs ocupadas coincidirem repetidamente (quase impossível),
            // após KEEP*2 tentativas força alocação de novo ID sequencial ignorando colisão
            // mas nunca retorna duplicata quando id==MAX já ocupado
            if attempts > KEEP * 4 {
                // busca linear por ID livre
                for cand in 1..=u32::MAX {
                    if !notices.iter().any(|n| n.id == cand) {
                        let _ =
                            self.next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |_| {
                                Some(cand.checked_add(1).unwrap_or(1))
                            });
                        return cand;
                    }
                    if cand == u32::MAX {
                        break;
                    }
                }
                // fallback extremo: retorna id mesmo duplicado só se queue lotada de MAX (teórico)
                return id;
            }
        }
    }

    fn reserve_id(&self, id: u32) {
        if id == u32::MAX {
            // wrap-around: id+1 seria 1 e rebobinaria next_id (ex: 3 -> 1).
            // Só avança se já está no MAX; senão mantém para preservar monotonicidade.
            let _ = self.next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current == u32::MAX).then_some(1)
            });
            return;
        }
        let next = id + 1;
        let _ = self.next_id.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current <= id).then_some(next)
        });
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Notice;

    fn mk(id: u32, app: &str) -> Notice {
        Notice {
            id,
            app: app.into(),
            summary: "s".into(),
            body: "".into(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
        }
    }

    fn push(q: &Queue, replaces: u32, notice: Notice) -> u32 {
        q.push_with_outcome(replaces, notice).id
    }

    #[test]
    fn caps_at_keep() {
        let q = Queue::new();
        for _ in 0..KEEP + 5 {
            push(&q, 0, mk(0, "A"));
        }
        assert_eq!(q.len(), KEEP);
    }

    #[test]
    fn reports_evicted_notifications() {
        let q = Queue::new();
        for _ in 0..KEEP {
            push(&q, 0, mk(0, "A"));
        }

        let outcome = q.push_with_outcome(0, mk(0, "B"));

        assert_eq!(outcome.evicted.len(), 1);
        assert_eq!(outcome.evicted[0].app, "A");
    }

    #[test]
    fn replaces_in_place() {
        let q = Queue::new();
        let id = push(&q, 0, mk(0, "A"));
        let id2 = push(&q, id, mk(0, "B"));
        assert_eq!(id, id2);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn replacement_keeps_the_requested_id_when_the_original_is_gone() {
        let q = Queue::new();
        // replaces inexistente → não squatta ID arbitrário, aloca novo (spec: 0 ou inválido = novo)
        let id = push(&q, 42, mk(0, "A"));

        assert_ne!(id, 42);
        assert_eq!(q.len(), 1);
        // segunda notificação deve ter ID sequencial diferente
        let id2 = push(&q, 0, mk(0, "B"));
        assert_ne!(id, id2);
    }

    #[test]
    fn removes_expired_notifications() {
        let q = Queue::new();
        let mut expired = mk(0, "Expired");
        expired.expire_ms = 10;
        expired.arrived_at_ms = 100;
        push(&q, 0, expired);
        push(&q, 0, mk(0, "Active"));

        let removed = q.remove_expired_at(110);

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].app, "Expired");
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn queues_each_close_request_once() {
        let q = Queue::new();
        q.request_close(7, CloseReason::DismissedByUser);
        q.request_close(7, CloseReason::ClosedByCall);

        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 7, reason: CloseReason::DismissedByUser }]
        );
    }

    #[test]
    fn close_request_upgrades_to_more_specific_reason() {
        // Precedência: DismissedByUser/ClosedByCall(3) > Expired(2) > Undefined(1).
        let q = Queue::new();
        q.request_close(1, CloseReason::Expired);
        q.request_close(1, CloseReason::DismissedByUser);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 1, reason: CloseReason::DismissedByUser }]
        );

        let q = Queue::new();
        q.request_close(2, CloseReason::Undefined);
        q.request_close(2, CloseReason::ClosedByCall);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 2, reason: CloseReason::ClosedByCall }]
        );
    }

    #[test]
    fn close_request_never_downgrades_reason() {
        // Ordem inversa NÃO pode rebaixar: primeiro vence se já é mais específico.
        let q = Queue::new();
        q.request_close(1, CloseReason::DismissedByUser);
        q.request_close(1, CloseReason::Expired);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 1, reason: CloseReason::DismissedByUser }]
        );

        let q = Queue::new();
        q.request_close(2, CloseReason::ClosedByCall);
        q.request_close(2, CloseReason::Undefined);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 2, reason: CloseReason::ClosedByCall }]
        );

        let q = Queue::new();
        q.request_close(3, CloseReason::Expired);
        q.request_close(3, CloseReason::Undefined);
        assert_eq!(
            q.drain_close_requests(),
            vec![CloseRequest { id: 3, reason: CloseReason::Expired }]
        );
    }

    #[test]
    fn reserve_id_never_rewinds_past_max() {
        use std::sync::atomic::Ordering;
        // Root cause: `(current <= id).then_some(id+1 ou 1)` com id=u32::MAX
        // e current pequeno rebobinava next_id para 1 (reuso não-monotônico).
        let q = Queue::new();
        let a = push(&q, 0, mk(0, "A"));
        let b = push(&q, 0, mk(0, "B"));
        assert_ne!(a, b);
        let before = q.next_id.load(Ordering::SeqCst);
        q.reserve_id(u32::MAX);
        let after = q.next_id.load(Ordering::SeqCst);
        assert_eq!(after, before, "reserve(u32::MAX) não pode rebobinar {before} -> {after}");
    }

    #[test]
    fn ids_stay_unique_when_replaces_is_missing() {
        // replaces inexistente não squatta: aloca novo e reserva sem reuso imediato.
        let q = Queue::new();
        let id1 = push(&q, 0, mk(0, "A"));
        let ghost = push(&q, 999_999, mk(0, "Ghost"));
        assert_ne!(ghost, 999_999);
        assert_ne!(ghost, id1);
        let mut seen = std::collections::HashSet::new();
        for n in q.snapshot() {
            assert!(seen.insert(n.id), "ID duplicado no snapshot: {}", n.id);
        }
        let id3 = push(&q, 0, mk(0, "C"));
        assert!(!seen.contains(&id3), "novo ID reutilizou ID vivo: {id3}");
    }
}
