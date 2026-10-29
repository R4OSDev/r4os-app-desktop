#include <r4os/r4os.h>
#include <r4os/app_gui.h>
static void text(R4App*a,const char*s){r4sys_write_cstr(&a->system,s);}
static void number(R4App*a,int64_t v){uint8_t b[24];uint32_t n=0;if(v<0){text(a,"-");v=-v;}do{b[n++]=(uint8_t)('0'+v%10);v/=10;}while(v);while(n)r4sys_write(&a->system,&b[--n],1);}
static void field(R4App*a,const char*s,int64_t n){text(a,s);number(a,n);}
static int info(R4App*a,R4Window*w,uint32_t down,uint32_t up){R4GuiWindowInfo i={0};int rc=r4desk_gui_window_info(&a->desktop,&i);if(rc<0)return rc;text(a,"W30 info");field(a," window=",i.window_id);field(a," x=",i.client_x);field(a," y=",i.client_y);field(a," w=",i.client_w);field(a," h=",i.client_h);field(a," down=",down);field(a," up=",up);text(a,"\n");(void)w;return 0;}
int32_t r4_app_main(R4App*a){
 R4Window w;if(!r4_window_open(a,0,0,&w)||r4_window_set_title(&w,"Wheel snapshot30")<0)return 1;
 uint32_t down=0,up=0;int dirty=1;if(info(a,&w,down,up))return 2;
 for(;;){
  if(dirty){R4PaintContext p;if(!r4_window_begin_paint(&w,&p))return 3;R4Canvas c=r4_paint_canvas(&p);if(r4_canvas_clear(c,0x193343)<0||r4_canvas_text(c,12,15,"Combined wheel + button packets",0xffffff,0x193343)<0||r4_paint_present(&p)<0)return 4;dirty=0;}
  R4MessageNext n=r4_window_wait_message(&w,r4_timeout_forever());if(n.state!=R4_MESSAGE_NEXT_MESSAGE)continue;R4Message m=n.message;
  if(m.kind==R4_MESSAGE_CLOSE)break;
  if(m.kind==R4_MESSAGE_MOUSE){R4MouseMessage v=m.value.mouse;if(v.action==R4_MOUSE_DOWN)down++;if(v.action==R4_MOUSE_UP)up++;if(v.action!=R4_MOUSE_MOVE){text(a,"W30 mouse");field(a," action=",v.action);field(a," x=",v.x);field(a," y=",v.y);field(a," buttons=",v.buttons);text(a,"\n");}}
  if(m.kind==R4_MESSAGE_RESIZE)dirty=1;
  if(m.kind==R4_MESSAGE_KEY){uint32_t k=m.value.key.codepoint;if(k=='s'||k=='S'){if(info(a,&w,down,up))return 5;}if(k=='q'||k=='Q')break;}
 }
 if(info(a,&w,down,up))return 6;text(a,"W30 finished");field(a," down=",down);field(a," up=",up);text(a,"\n");return down==2&&up==2?0:7;
}
