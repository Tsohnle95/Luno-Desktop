//! Safe scan scheduling and publication rules for filesystem changes.

use super::watch::WatchBatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileAction { Noop, Incremental, FullScan }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanGeneration { pub generation: u64, pub action: ReconcileAction }

#[derive(Debug, Default)]
pub struct Reconciler { next: u64, active: Option<ScanGeneration>, pending: bool }

impl Reconciler {
    pub fn request(&mut self, batch: Option<&WatchBatch>) -> ScanGeneration {
        let action = match batch { None => ReconcileAction::FullScan, Some(b) if b.paths.is_empty() => ReconcileAction::Noop, Some(b) if b.full_reconcile => ReconcileAction::FullScan, Some(_) => ReconcileAction::Incremental };
        if action == ReconcileAction::Noop { return ScanGeneration { generation: self.next, action }; }
        if let Some(active) = self.active { self.pending = true; return active; }
        self.next += 1;
        let scan = ScanGeneration { generation: self.next, action };
        self.active = Some(scan);
        scan
    }

    pub fn finish(&mut self, scan: ScanGeneration) -> bool {
        if self.active != Some(scan) { return false; }
        self.active = None;
        true
    }

    pub fn take_pending(&mut self) -> bool { let value = self.pending; self.pending = false; value }
    pub fn active(&self) -> Option<ScanGeneration> { self.active }
    pub fn generation(&self) -> u64 { self.next }
}
