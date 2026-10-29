/* Explicit test producer. Pacing belongs only to the adverse consumer case. */
#include <r4os/r4os.h>
static void number(R4App *a,uint64_t value){uint8_t b[24];uint32_t n=0;do{b[n++]=(uint8_t)('0'+value%10);value/=10;}while(value);while(n)(void)r4sys_write(&a->system,&b[--n],1);}
static int host(R4App *a){
 R4ProgramInventoryCursor cursor={.version=1,.size=sizeof cursor};
 R4ProgramInventorySummary summary={.version=1,.size=sizeof summary};
 if(r4sys_program_inventory_begin(&a->system,&cursor,&summary)<0)return 4;
 uint32_t found=0;
 for(uint32_t page_index=0;page_index<64;++page_index){
  R4ProgramInstanceSnapshot rows[8];R4ProgramInventoryPageInfo page={.version=1,.size=sizeof page};
  if(r4sys_program_inventory_programs(&a->system,&cursor,rows,8,&page)<0)return 5;
  for(uint32_t i=0;i<page.returned;++i)if(rows[i].info.flags&R4OS_PROGRAM_INSTANCE_FLAG_DESKTOP_HOST){
   ++found;r4sys_write_cstr(&a->system,"Q32 HOST id=");number(a,rows[i].handle.instance_id);
   r4sys_write_cstr(&a->system," generation=");number(a,rows[i].handle.generation);r4sys_write_cstr(&a->system,"\n");
  }
  if(!page.has_more)return found==1?0:6;
 }
 return 7;
}
int32_t r4_app_main(R4App *a){
 uint64_t length=0;const uint8_t *args=r4_app_args(a,&length);if(!length)return 2;
 if(args[0]=='H')return host(a);
 uint32_t count=args[0]=='B'?300:1,key=args[0]=='E'?27:args[0]=='Z'?'z':'x';
 R4RemoteInputEvent e={.magic=R4_REMOTE_INPUT_MAGIC,.version=R4_REMOTE_INPUT_VERSION,.kind=R4_REMOTE_INPUT_KIND_KEY_DOWN,.key=key};
 for(uint32_t i=0;i<count;++i){
  int32_t rc=r4desk_remote_input_push(&a->desktop,&e);if(rc<0){r4sys_write_cstr(&a->system,"Q32 producer rejected\n");return 3;}
  if((i&3)==3)r4sys_sleep_ticks(&a->system,1);
 }
 r4sys_write_cstr(&a->system,"Q32 producer accepted\n");return 0;
}
