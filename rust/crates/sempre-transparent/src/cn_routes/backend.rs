use std::future::Future;

use super::{
    model::{Endpoint, Intent, Route},
    platform,
};
use crate::{TransparentError, command};

pub(crate) trait Backend: Default + Send + Sync + 'static {
    fn batch_size(&self) -> usize {
        128
    }
    fn discover<'a>(
        &'a self,
        runner: &'a dyn command::Runner,
        intent: &'a Intent,
    ) -> impl Future<Output = Result<(Vec<Endpoint>, Endpoint), TransparentError>> + Send + 'a;
    fn preflight<'a>(
        &'a self,
        runner: &'a dyn command::Runner,
        routes: &'a [Route],
    ) -> impl Future<Output = Result<(), TransparentError>> + Send + 'a;
    fn change<'a>(
        &'a self,
        runner: &'a dyn command::Runner,
        routes: &'a [Route],
        add: bool,
    ) -> impl Future<Output = Result<(), TransparentError>> + Send + 'a;
    fn policy<'a>(
        &'a self,
        runner: &'a dyn command::Runner,
        add: bool,
    ) -> impl Future<Output = Result<(), TransparentError>> + Send + 'a;
}

#[derive(Default)]
pub(crate) struct Native;

impl Backend for Native {
    async fn discover(
        &self,
        runner: &dyn command::Runner,
        intent: &Intent,
    ) -> Result<(Vec<Endpoint>, Endpoint), TransparentError> {
        platform::discover(runner, &intent.tun_addresses).await
    }
    async fn preflight(
        &self,
        runner: &dyn command::Runner,
        routes: &[Route],
    ) -> Result<(), TransparentError> {
        platform::preflight(runner, routes).await
    }
    async fn change(
        &self,
        runner: &dyn command::Runner,
        routes: &[Route],
        add: bool,
    ) -> Result<(), TransparentError> {
        platform::change(runner, routes, add).await
    }
    async fn policy(
        &self,
        runner: &dyn command::Runner,
        add: bool,
    ) -> Result<(), TransparentError> {
        platform::policy(runner, add).await
    }
}
