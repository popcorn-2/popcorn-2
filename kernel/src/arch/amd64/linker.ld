ENTRY(_start)

PHDRS {
    headers PT_PHDR PHDRS ;
    realmode PT_LOAD FLAGS(0x010007) ;
    rodata PT_LOAD FLAGS(0x4) ;
    text PT_LOAD FLAGS(0x5) ;
    data PT_LOAD FLAGS(0x6) ;
    dynamic PT_DYNAMIC ;
}

SECTIONS {
	. = 0xFFFFFFFF80000000;

	.dynsym :
    {
        *.dynsym*
    } :rodata

    .gnu.hash :
    {
        *.gnu.hash*
    }

    .hash :
    {
        *.hash*
    }

    .dynstr :
    {
        *.dynstr*
    }

    .rela.dyn :
    {
        *(.rela.init)
        *(.rela.text .rela.text.* .rela.gnu.linkonce.t.*)
        *(.rela.fini)
        *(.rela.rodata .rela.rodata.* .rela.gnu.linkonce.r.*)
        *(.rela.data .rela.data.* .rela.gnu.linkonce.d.*)
        *(.rela.tdata .rela.tdata.* .rela.gnu.linkonce.td.*)
        *(.rela.tbss .rela.tbss.* .rela.gnu.linkonce.tb.*)
        *(.rela.ctors)
        *(.rela.dtors)
        *(.rela.got)
        *(.rela.bss .rela.bss.* .rela.gnu.linkonce.b.*)
        *(.rela.ldata .rela.ldata.* .rela.gnu.linkonce.l.*)
        *(.rela.lbss .rela.lbss.* .rela.gnu.linkonce.lb.*)
        *(.rela.lrodata .rela.lrodata.* .rela.gnu.linkonce.lr.*)
        *(.rela.ifunc)
    }

    .rela.plt :
    {
        *(.rela.plt)
        *(.rela.iplt)
    }

	.data.rel.ro :
    {
        *(.data.rel.ro*)
    } :rodata

	.rodata :
	{
		*(.rodata*)
	} :rodata

	. = ALIGN(4K);
    .text :
    {
        *(.text*)
    } :text

    .plt :
    {
        *(.plt*)
    }

	. = ALIGN(4K);
	.data :
	{
		*(.data)
	} :data

	.got.plt :
	{
	    *(.got.plt*)
	}

	.bss :
	{
		*(COMMON)
		*(.bss*)
	}

	.dynamic :
	{
	    *.dynamic*
	} :data :dynamic

	/DISCARD/ : {
		*(.debug_gdb_scripts*)
	}
}
