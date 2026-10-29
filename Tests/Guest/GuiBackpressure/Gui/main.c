/* Public GUI consumer for native queue retention and cancellation. No provider
 * replacement: self-push fills the real ring; Remote/PS2 supplies real input. */
#include <r4os/app_gui.h>
static void text(R4App *a,const char *s){(void)r4sys_write_cstr(&a->system,s);}
static void number(R4App *a,uint64_t value){uint8_t b[24];uint32_t n=0;do{b[n++]=(uint8_t)('0'+value%10);value/=10;}while(value);while(n)(void)r4sys_write(&a->system,&b[--n],1);}
static void field(R4App *a,const char *s,uint64_t value){text(a,s);number(a,value);}
static uint32_t held_keys(const uint8_t *held){uint32_t n=0;for(uint32_t i=0;i<512;++i)n+=held[i]!=0;return n;}
static int fill(R4App *a,int32_t id,uint32_t *count){
 if(!a->desktop.table->gui_push_event)return 0;
 R4DeskGuiPushEventFn push=(R4DeskGuiPushEventFn)(uintptr_t)a->desktop.table->gui_push_event;
 R4GuiEvent e={.kind=R4_GUI_RAW_EVENT_KEY_DOWN,.window_id=id,.key='~',.tick=r4_app_ticks(a)};
 int32_t code=0;for(uint32_t i=0;i<65 && code==0;++i){code=push(a->context->instance_id,&e);if(code==0)++*count;}
 return code==-3 && *count==63;
}
int32_t r4_app_main(R4App *a){
 uint64_t arg_length=0;const uint8_t *args=r4_app_args(a,&arg_length);char mode=arg_length?(char)args[0]:'D';
 if(mode!='D'&&mode!='O'&&mode!='F'&&mode!='C'&&mode!='B'&&mode!='K'&&mode!='R'&&mode!='P')return 2;
 R4Window window;if(!r4_window_open(a,0,0,&window)||r4_window_set_title(&window,"GUI backpressure32")<0)return 3;
 R4PaintContext paint;if(!r4_window_begin_paint(&window,&paint))return 4;
 R4Canvas canvas=r4_paint_canvas(&paint);
 if(r4_canvas_clear(canvas,0x233748)<0||r4_canvas_text(canvas,12,15,"Native queue retention / explicit cancel",0xffffff,0x233748)<0||r4_paint_present(&paint)<0)return 5;
 R4GuiPointerWindowIdentity identity={0};if(r4_window_pointer_identity(&window,&identity)<0)return 6;
 text(a,"Q32 armed mode=");(void)r4sys_write(&a->system,(const uint8_t *)&mode,1);
 field(a," owner=",identity.owner.instance_id);field(a," generation=",identity.owner.generation);
 field(a," window=",(uint32_t)identity.window_id);field(a," epoch=",identity.epoch);text(a,"\n");
 /* Exact host admission is Desktop-only. Public C must retain native errors
  * and leave output untouched, including 64-bit owner/window identity. */
 R4GuiHostInputRequest request={.window=identity,.operation=R4OS_GUI_HOST_INPUT_OPERATION_QUERY};request.window.epoch=0;
 R4GuiPointerWindowIdentity output;for(uint32_t i=0;i<sizeof output;++i)((uint8_t *)&output)[i]=0xa5;
 if(r4desk_gui_host_input(&a->desktop,&request,&output)!=R4OS_GUI_HOST_INPUT_ERROR_CONTEXT)return 7;
 for(uint32_t i=0;i<sizeof output;++i)if(((uint8_t *)&output)[i]!=0xa5)return 7;
 uint32_t down=0,up=0,filled=0,drained=0,keys=0,esc=0,physical_down=0,physical_up=0,physical_reset=0;
 uint32_t resets=0,reason=0,focus_lost=0,closed=0,clicks=0,stale=0,capture_active=0,capture_terminal=0;
 uint8_t held[512]={0};int mouse_held=0,full=0,done=0;
 uint64_t deadline=r4_app_ticks(a)+(uint64_t)r4_gui_monotonic_hz(a)*15;
 while(!done && r4_app_ticks(a)<deadline){
  R4Timeout timeout={.kind=R4OS_TIMEOUT_KIND_FINITE,.nanoseconds=UINT64_C(200000000)};
  R4MessageNext next=r4_window_wait_message(&window,timeout);
  if(next.state!=R4_MESSAGE_NEXT_MESSAGE)continue;
  R4Message m=next.message;
  if(m.kind==R4_MESSAGE_MOUSE){
   if(m.value.mouse.action==R4_MOUSE_DOWN){++down;mouse_held=1;
    if(mode=='P'&&!capture_active&&!full){R4GuiPointerCaptureHandle token={0};if(r4_window_pointer_capture(&window,&identity,R4OS_GUI_POINTER_MODE_DRAG,&token)<0)return 8;}
    else if(!full&&mode!='R'){
     if(!fill(a,identity.window_id,&filled))return 9;full=1;
     text(a,"Q32 full");field(a," filled=",filled);text(a,"\n");
     r4sys_sleep_ticks(&a->system,(uint64_t)r4_gui_monotonic_hz(a)*2);text(a,"Q32 resumed\n");
    }
   }else if(m.value.mouse.action==R4_MOUSE_UP){++up;if(mouse_held)++clicks;mouse_held=0;}
  }else if(m.kind==R4_MESSAGE_KEY){
   uint32_t key=m.value.key.codepoint;
   if(key=='~')++drained;
   if(key=='a'||key=='A')++keys;
   if(key==27)++esc;
   if(mode=='R'&&key!='z')++stale;
   if((mode=='O'||mode=='R')&&key=='z')done=1;
  }else if(m.kind==R4_MESSAGE_INPUT_RESET){
   ++resets;reason=m.value.input_reset.reason;mouse_held=0;for(uint32_t i=0;i<512;++i)held[i]=0;
   if(mode=='B'&&reason==R4OS_GUI_INPUT_RESET_REASON_DESKTOP_CHANGED)done=1;
  }else if(m.kind==R4_MESSAGE_FOCUS_LOST){++focus_lost;if(mode=='F')done=1;
  }else if(m.kind==R4_MESSAGE_POINTER_CAPTURE){
   if(m.value.capture.state==R4OS_GUI_POINTER_STATE_ACTIVE){++capture_active;
    if(mode=='P'&&!full){if(!fill(a,identity.window_id,&filled))return 9;full=1;
     text(a,"Q32 full");field(a," filled=",filled);text(a,"\n");
     r4sys_sleep_ticks(&a->system,(uint64_t)r4_gui_monotonic_hz(a)*2);text(a,"Q32 resumed\n");}
   }else if(m.value.capture.state==R4OS_GUI_POINTER_STATE_CANCELLED||m.value.capture.state==R4OS_GUI_POINTER_STATE_RELEASED){++capture_terminal;mouse_held=0;}
  }else if(m.kind==R4_MESSAGE_UNKNOWN){
   R4GuiEvent raw=m.value.unknown;
   if(raw.kind==R4ABI_GUI_EVENT_KIND_PHYSICAL_KEY_DOWN||raw.kind==R4ABI_GUI_EVENT_KIND_PHYSICAL_KEY_UP){
    if(raw.key==0||raw.key>=512)return 10;
    if(raw.kind==R4ABI_GUI_EVENT_KIND_PHYSICAL_KEY_DOWN){++physical_down;held[raw.key]=1;}else{++physical_up;held[raw.key]=0;}
   }else if(raw.kind==R4ABI_GUI_EVENT_KIND_PHYSICAL_KEY_RESET){++physical_reset;for(uint32_t i=0;i<512;++i)held[i]=0;}
  }else if(m.kind==R4_MESSAGE_CLOSE){closed=1;done=1;}
  if(mode=='D'&&up==1&&keys==1&&esc==1&&physical_down>=1&&physical_up>=1&&held_keys(held)==0&&drained==filled)done=1;
 }
 int pass=done&&mouse_held==0&&held_keys(held)==0;
 if(mode=='D')pass=pass&&down==1&&up==1&&clicks==1&&keys==1&&esc==1&&filled==63&&drained==63&&resets==0;
 if(mode=='O')pass=pass&&resets>=1&&reason==R4OS_GUI_INPUT_RESET_REASON_OVERFLOW&&drained==0&&clicks==0;
 if(mode=='F')pass=pass&&resets>=1&&reason==R4OS_GUI_INPUT_RESET_REASON_FOCUS_LOST&&focus_lost>=1&&clicks==0;
 if(mode=='C')pass=pass&&closed&&resets>=1&&reason==R4OS_GUI_INPUT_RESET_REASON_WINDOW_CLOSED&&clicks==0;
 if(mode=='B')pass=pass&&resets>=1&&reason==R4OS_GUI_INPUT_RESET_REASON_DESKTOP_CHANGED&&clicks==0;
 if(mode=='R')pass=pass&&stale==0&&down==0&&up==0&&resets==0;
 if(mode=='P')pass=pass&&closed&&capture_active==1&&capture_terminal==1&&clicks==0;
 if(mode=='K')pass=0; /* This mode must end via the external exact-owner kill. */
 text(a,"Q32 result mode=");(void)r4sys_write(&a->system,(const uint8_t *)&mode,1);
 field(a," down=",down);field(a," up=",up);field(a," filled=",filled);field(a," drained=",drained);
 field(a," key=",keys);field(a," esc=",esc);field(a," physical_down=",physical_down);field(a," physical_up=",physical_up);field(a," physical_reset=",physical_reset);
 field(a," reset=",resets);field(a," reason=",reason);field(a," focus_lost=",focus_lost);field(a," close=",closed);field(a," clicks=",clicks);
 field(a," held=",held_keys(held)+(mouse_held!=0));field(a," stale=",stale);field(a," capture_active=",capture_active);field(a," capture_terminal=",capture_terminal);field(a," pass=",pass!=0);text(a,"\n");
 return pass?0:11;
}
