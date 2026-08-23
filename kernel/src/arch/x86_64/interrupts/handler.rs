pub struct Handler<const NUM: u8>;

macro_rules! irq_handler {
    ($num:literal error) => {
	    impl Handler<$num> {
		    #[unsafe(naked)]
		    pub unsafe extern "custom" fn handler() {
			    ::core::arch::naked_asm!(
					"push {num}",
					"jmp {stub}",
					num = const $num,
				    stub = sym super::x86_64_interrupt_stub,
			    );
		    }
	    }
    };

    ($num:literal) => {
	    impl Handler<$num> {
		    #[unsafe(naked)]
		    pub unsafe extern "custom" fn handler() {
			    ::core::arch::naked_asm!(
				    "push 0",
					"push {num}",
					"jmp {stub}",
					num = const $num,
				    stub = sym super::x86_64_interrupt_stub,
			    );
		    }
	    }
    };
}

irq_handler!(0);
irq_handler!(1);
irq_handler!(2);
irq_handler!(3);
irq_handler!(4);
irq_handler!(5);
irq_handler!(6);
irq_handler!(7);
irq_handler!(8 error);
irq_handler!(9);
irq_handler!(10 error);
irq_handler!(11 error);
irq_handler!(12 error);
irq_handler!(13 error);
irq_handler!(14 error);
irq_handler!(15);
irq_handler!(16);
irq_handler!(17 error);
irq_handler!(18);
irq_handler!(19);
irq_handler!(20);
irq_handler!(21 error);
irq_handler!(22);
irq_handler!(23);
irq_handler!(24);
irq_handler!(25);
irq_handler!(26);
irq_handler!(27);
irq_handler!(28);
irq_handler!(29 error);
irq_handler!(30 error);
irq_handler!(31);
irq_handler!(32);
irq_handler!(33);
irq_handler!(34);
irq_handler!(35);
irq_handler!(36);
irq_handler!(37);
irq_handler!(38);
irq_handler!(39);
irq_handler!(40);
irq_handler!(41);
irq_handler!(42);
irq_handler!(43);
irq_handler!(44);
irq_handler!(45);
irq_handler!(46);
irq_handler!(47);
irq_handler!(48);
irq_handler!(49);
irq_handler!(50);
irq_handler!(51);
irq_handler!(52);
irq_handler!(53);
irq_handler!(54);
irq_handler!(55);
irq_handler!(56);
irq_handler!(57);
irq_handler!(58);
irq_handler!(59);
irq_handler!(60);
irq_handler!(61);
irq_handler!(62);
irq_handler!(63);
irq_handler!(64);
irq_handler!(65);
irq_handler!(66);
irq_handler!(67);
irq_handler!(68);
irq_handler!(69);
irq_handler!(70);
irq_handler!(71);
irq_handler!(72);
irq_handler!(73);
irq_handler!(74);
irq_handler!(75);
irq_handler!(76);
irq_handler!(77);
irq_handler!(78);
irq_handler!(79);
irq_handler!(80);
irq_handler!(81);
irq_handler!(82);
irq_handler!(83);
irq_handler!(84);
irq_handler!(85);
irq_handler!(86);
irq_handler!(87);
irq_handler!(88);
irq_handler!(89);
irq_handler!(90);
irq_handler!(91);
irq_handler!(92);
irq_handler!(93);
irq_handler!(94);
irq_handler!(95);
irq_handler!(96);
irq_handler!(97);
irq_handler!(98);
irq_handler!(99);
irq_handler!(100);
irq_handler!(101);
irq_handler!(102);
irq_handler!(103);
irq_handler!(104);
irq_handler!(105);
irq_handler!(106);
irq_handler!(107);
irq_handler!(108);
irq_handler!(109);
irq_handler!(110);
irq_handler!(111);
irq_handler!(112);
irq_handler!(113);
irq_handler!(114);
irq_handler!(115);
irq_handler!(116);
irq_handler!(117);
irq_handler!(118);
irq_handler!(119);
irq_handler!(120);
irq_handler!(121);
irq_handler!(122);
irq_handler!(123);
irq_handler!(124);
irq_handler!(125);
irq_handler!(126);
irq_handler!(127);
irq_handler!(128);
irq_handler!(129);
irq_handler!(130);
irq_handler!(131);
irq_handler!(132);
irq_handler!(133);
irq_handler!(134);
irq_handler!(135);
irq_handler!(136);
irq_handler!(137);
irq_handler!(138);
irq_handler!(139);
irq_handler!(140);
irq_handler!(141);
irq_handler!(142);
irq_handler!(143);
irq_handler!(144);
irq_handler!(145);
irq_handler!(146);
irq_handler!(147);
irq_handler!(148);
irq_handler!(149);
irq_handler!(150);
irq_handler!(151);
irq_handler!(152);
irq_handler!(153);
irq_handler!(154);
irq_handler!(155);
irq_handler!(156);
irq_handler!(157);
irq_handler!(158);
irq_handler!(159);
irq_handler!(160);
irq_handler!(161);
irq_handler!(162);
irq_handler!(163);
irq_handler!(164);
irq_handler!(165);
irq_handler!(166);
irq_handler!(167);
irq_handler!(168);
irq_handler!(169);
irq_handler!(170);
irq_handler!(171);
irq_handler!(172);
irq_handler!(173);
irq_handler!(174);
irq_handler!(175);
irq_handler!(176);
irq_handler!(177);
irq_handler!(178);
irq_handler!(179);
irq_handler!(180);
irq_handler!(181);
irq_handler!(182);
irq_handler!(183);
irq_handler!(184);
irq_handler!(185);
irq_handler!(186);
irq_handler!(187);
irq_handler!(188);
irq_handler!(189);
irq_handler!(190);
irq_handler!(191);
irq_handler!(192);
irq_handler!(193);
irq_handler!(194);
irq_handler!(195);
irq_handler!(196);
irq_handler!(197);
irq_handler!(198);
irq_handler!(199);
irq_handler!(200);
irq_handler!(201);
irq_handler!(202);
irq_handler!(203);
irq_handler!(204);
irq_handler!(205);
irq_handler!(206);
irq_handler!(207);
irq_handler!(208);
irq_handler!(209);
irq_handler!(210);
irq_handler!(211);
irq_handler!(212);
irq_handler!(213);
irq_handler!(214);
irq_handler!(215);
irq_handler!(216);
irq_handler!(217);
irq_handler!(218);
irq_handler!(219);
irq_handler!(220);
irq_handler!(221);
irq_handler!(222);
irq_handler!(223);
irq_handler!(224);
irq_handler!(225);
irq_handler!(226);
irq_handler!(227);
irq_handler!(228);
irq_handler!(229);
irq_handler!(230);
irq_handler!(231);
irq_handler!(232);
irq_handler!(233);
irq_handler!(234);
irq_handler!(235);
irq_handler!(236);
irq_handler!(237);
irq_handler!(238);
irq_handler!(239);
irq_handler!(240);
irq_handler!(241);
irq_handler!(242);
irq_handler!(243);
irq_handler!(244);
irq_handler!(245);
irq_handler!(246);
irq_handler!(247);
irq_handler!(248);
irq_handler!(249);
irq_handler!(250);
irq_handler!(251);
irq_handler!(252);
irq_handler!(253);
irq_handler!(254);
irq_handler!(255);
