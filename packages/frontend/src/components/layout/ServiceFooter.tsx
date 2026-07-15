"use client";

import styled from "styled-components";

export function ServiceFooter() {
  return (
    <Footer aria-label="About tokens.ci">
      <Inner>
        <Statement>
          <ProjectLink
            href="https://github.com/missuo/tokens"
            target="_blank"
            rel="noopener noreferrer"
          >
            tokens.ci
          </ProjectLink>{" "}
          is an open-source project built on top of{" "}
          <ProjectLink
            href="https://github.com/junhoyeo/tokscale"
            target="_blank"
            rel="noopener noreferrer"
          >
            Tokscale
          </ProjectLink>
          .
        </Statement>
      </Inner>
    </Footer>
  );
}

const Footer = styled.footer`
  width: 100%;
  border-top: 1px solid var(--service-border);
  background: var(--service-surface);
`;

const Inner = styled.div`
  width: 100%;
  max-width: 1500px;
  margin: 0 auto;
  padding: 18px 32px;
  text-align: center;

  @media (max-width: 520px) {
    padding: 16px;
  }
`;

const Statement = styled.p`
  margin: 0;
  color: var(--service-text-muted);
  font-size: 0.8125rem;
  line-height: 1.5;
`;

const ProjectLink = styled.a`
  color: var(--service-text);
  font-weight: 600;
  text-decoration: none;

  &:hover {
    color: var(--service-accent-hover);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  &:focus-visible {
    border-radius: 4px;
    outline: 2px solid var(--service-focus);
    outline-offset: 3px;
  }
`;
