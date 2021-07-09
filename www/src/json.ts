export interface AccountStatus {
    result_type: string;
    message: Message;
}

export interface Message {
    state: State;
    notifications: Notification[];
}

export interface State {
    context: Context;
    is_fully_verified: boolean;
    completion_timestamp?: any;
    fields: Field[];
}

export interface Context {
    address: string;
    chain: string;
}

export interface Field {
    value: FieldValue;
    challenge: Challenge;
    failed_attempts: number;
}

export interface FieldValue {
    type: string;
    value: string;
}

export interface Challenge {
    challenge_type: string;
    content: Content;
}

export interface Content {
    passed?: boolean;
    expected: Expected;
    second?: Expected;
}

export interface Expected {
    value: string;
    is_verified: boolean;
}

export interface Notification {
    type: string;
    value: any[];
}

export interface Value2 {
    address: string;
    chain: string;
    type: string;
    value: string;
}
