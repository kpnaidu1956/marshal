export interface Message {
 id: string
 organization_id: string
 sender_id: string | null
 recipient_id: string | null
 subject: string | null
 content: string
 is_read: boolean | null
 is_archived: boolean | null
 created_at: string | null
 updated_at: string | null
}

export interface Conversation {
 id: string
 organization_id: string
 title: string | null
 created_at: string | null
 updated_at: string | null
}

export interface ChatMessage {
 id: string
 conversation_id: string
 organization_id: string
 role: string
 content: string
 created_at: string | null
}
