use std::task::{Context, Poll};

use af_adapter::UpstreamBody;
use af_domain::UpstreamError;
use af_protocol::CanonicalStreamEvent;

use crate::{
    conversation_decoder::ConversationDecoder, error::map_adaptor_error,
    generation_stream::StreamSource, tool_conversation_stream::ToolConversationStream,
};

/// 字节解码与受控续答共用同一个事件入口；下游编码、用量和收尾只执行一次。
pub(crate) enum ConversationSource {
    Wire {
        source: StreamSource,
        decoder: Box<ConversationDecoder>,
    },
    Tools(Box<ToolConversationStream>),
}

impl ConversationSource {
    pub(crate) fn wire(body: UpstreamBody, decoder: ConversationDecoder) -> Self {
        Self::Wire {
            source: body.into(),
            decoder: Box::new(decoder),
        }
    }

    pub(crate) fn poll_next(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Vec<CanonicalStreamEvent>, UpstreamError>>> {
        match self {
            Self::Tools(stream) => stream.poll_next(context),
            Self::Wire { source, decoder } => match source.poll_next(context) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(decoder.push(&bytes))),
                Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(map_adaptor_error(error)))),
                Poll::Ready(None) => match decoder.finish() {
                    Ok(events) if events.is_empty() => Poll::Ready(None),
                    result => Poll::Ready(Some(result)),
                },
            },
        }
    }
}
