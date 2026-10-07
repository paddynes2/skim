import {invoke} from "@tauri-apps/api/core";
export interface QueueFailure {id:number;kind:string;action:string;accountEmail:string;createdAt:number;retryable:boolean}
export interface QueueStatus {pending:number;scheduled:number;failed:number;failures:QueueFailure[]}
export const syncStatusApi={status:(accountId:string|null)=>invoke<QueueStatus>("fork_mail_queue_status",{accountId}),retry:(id:number)=>invoke<void>("fork_mail_retry_flag",{id})};
