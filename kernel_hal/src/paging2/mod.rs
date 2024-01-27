pub type KTable = crate::arch::amd64::paging2::KTable;
pub type TTable = crate::arch::amd64::paging2::TTable;

pub unsafe fn construct_tables() -> (KTable, TTable) {
	crate::arch::amd64::paging2::construct_tables()
}
