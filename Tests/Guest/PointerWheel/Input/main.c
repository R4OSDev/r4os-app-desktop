#include <r4os/r4os.h>
static int part(const uint8_t*s,uint64_t n,uint64_t*p,int32_t*out){int sign=1;if(*p<n&&s[*p]=='-'){sign=-1;(*p)++;}uint64_t start=*p,v=0;while(*p<n&&s[*p]>='0'&&s[*p]<='9'){v=v*10+s[(*p)++]-'0';if(v>INT32_MAX)return 0;}if(*p==start)return 0;if(*p<n&&s[(*p)++]!=':')return 0;*out=(int32_t)v*sign;return 1;}
int32_t r4_app_main(R4App*a){uint64_t n=0,p=0;const uint8_t*s=r4_app_args(a,&n);int32_t x,y,b,w;if(!s||!part(s,n,&p,&x)||!part(s,n,&p,&y)||!part(s,n,&p,&b)||!part(s,n,&p,&w)||b<0||b>7)return 2;
 R4RemoteInputEvent e={.magic=R4_REMOTE_INPUT_MAGIC,.version=R4_REMOTE_INPUT_VERSION,.kind=w?R4_REMOTE_INPUT_KIND_MOUSE_WHEEL:R4_REMOTE_INPUT_KIND_MOUSE_MOVE,.x=x,.y=y,.buttons=(uint32_t)b,.wheel=w};
 int rc=r4desk_remote_input_push(&a->desktop,&e);r4sys_write_cstr(&a->system,rc<0?"W30 input failed\n":"W30 input accepted\n");return rc<0?1:0;}
